//! Connecteur Twitch : chat en lecture/écriture via IRC sur WebSocket,
//! modération et infos de live via l'API Helix.

mod irc;

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use reqwest::Method;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use super::{ChatConnector, ChatEvent};
use crate::auth::{self, now_secs, TokenSet};
use crate::domain::{Channel, ModerationAction, ModerationKind, Platform, StreamInfo};
use crate::{Error, Result};

const IRC_URL: &str = "wss://irc-ws.chat.twitch.tv:443";
const HELIX_URL: &str = "https://api.twitch.tv/helix";
const DEFAULT_REASON: &str = "Modération Omnistream";

/// Appelé avec `(user_id, nouveaux jetons)` après chaque renouvellement, pour les persister.
pub type TokenHook = Arc<dyn Fn(&str, &TokenSet) + Send + Sync>;

#[derive(Clone)]
pub struct TwitchCreds {
    pub client_id: String,
    pub user_id: String,
    pub login: String,
    pub tokens: TokenSet,
}

struct ChannelHandle {
    task: JoinHandle<()>,
    outbox: mpsc::UnboundedSender<String>,
}

struct Inner {
    http: reqwest::Client,
    creds: Mutex<Option<TwitchCreds>>,
    channels: StdMutex<HashMap<String, ChannelHandle>>,
    user_ids: StdMutex<HashMap<String, String>>,
    hook: StdMutex<Option<TokenHook>>,
}

#[derive(Clone)]
pub struct TwitchConnector {
    inner: Arc<Inner>,
}

impl Default for TwitchConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl TwitchConnector {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                http: reqwest::Client::new(),
                creds: Mutex::new(None),
                channels: StdMutex::default(),
                user_ids: StdMutex::default(),
                hook: StdMutex::default(),
            }),
        }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.inner.http
    }

    pub async fn set_credentials(&self, creds: Option<TwitchCreds>) {
        *self.inner.creds.lock().await = creds;
    }

    /// Titre actuel de la chaîne du compte connecté.
    pub async fn channel_title(&self) -> Result<String> {
        let creds = self.inner.auth(false).await?;
        let v = self
            .inner
            .helix(
                Method::GET,
                "channels",
                &[("broadcaster_id", &creds.user_id)],
                None,
            )
            .await?;
        Ok(v["data"][0]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Met à jour le titre de la chaîne du compte connecté (scope `channel:manage:broadcast`).
    pub async fn set_channel_title(&self, title: &str) -> Result<()> {
        let creds = self.inner.auth(false).await?;
        self.inner
            .helix(
                Method::PATCH,
                "channels",
                &[("broadcaster_id", &creds.user_id)],
                Some(serde_json::json!({ "title": title })),
            )
            .await
            .map(|_| ())
            .map_err(|e| match e {
                Error::Other(m) if m.contains("403") || m.contains("401") => Error::Other(format!(
                    "{m} — déconnectez puis reconnectez Twitch pour accorder la permission de modifier le titre."
                )),
                other => other,
            })
    }

    pub async fn is_connected(&self) -> bool {
        self.inner.creds.lock().await.is_some()
    }

    pub fn on_token_refresh(&self, hook: TokenHook) {
        *self.inner.hook.lock().unwrap() = Some(hook);
    }
}

impl Inner {
    /// Identifiants valides (jeton renouvelé s'il expire dans moins d'une minute).
    async fn auth(&self, force_refresh: bool) -> Result<TwitchCreds> {
        let mut guard = self.creds.lock().await;
        let creds = guard.as_mut().ok_or(Error::NotAuthenticated("Twitch"))?;
        let expiring = creds.tokens.expires_at.is_some_and(|e| e - 60 < now_secs());
        if force_refresh || expiring {
            if let Some(refresh) = creds.tokens.refresh_token.clone() {
                let fresh = auth::twitch::refresh(&self.http, &creds.client_id, &refresh).await?;
                creds.tokens = fresh;
                if let Some(hook) = self.hook.lock().unwrap().clone() {
                    hook(&creds.user_id, &creds.tokens);
                }
            }
        }
        Ok(creds.clone())
    }

    async fn helix(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Value> {
        let mut attempt = 0;
        loop {
            let creds = self.auth(attempt > 0).await?;
            let mut req = self
                .http
                .request(method.clone(), format!("{HELIX_URL}/{path}"))
                .header("Client-Id", &creds.client_id)
                .bearer_auth(&creds.tokens.access_token)
                .query(query);
            if let Some(b) = &body {
                req = req.json(b);
            }
            let resp = req.send().await?;
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                attempt += 1;
                continue;
            }
            let text = resp.text().await?;
            let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if !status.is_success() {
                let msg = value["message"].as_str().unwrap_or(text.as_str());
                return Err(Error::Other(format!("Twitch ({status}): {msg}")));
            }
            return Ok(value);
        }
    }

    async fn user_id(&self, login: &str) -> Result<String> {
        let login = login.to_lowercase();
        if let Some(id) = self.user_ids.lock().unwrap().get(&login) {
            return Ok(id.clone());
        }
        let v = self
            .helix(Method::GET, "users", &[("login", &login)], None)
            .await?;
        let id = v["data"][0]["id"]
            .as_str()
            .ok_or_else(|| Error::Other(format!("chaîne Twitch introuvable: {login}")))?
            .to_string();
        self.user_ids.lock().unwrap().insert(login, id.clone());
        Ok(id)
    }
}

#[async_trait]
impl ChatConnector for TwitchConnector {
    fn platform(&self) -> Platform {
        Platform::Twitch
    }

    async fn connect(&self, channel: &Channel, tx: mpsc::Sender<ChatEvent>) -> Result<()> {
        self.disconnect(channel).await?;
        let (otx, orx) = mpsc::unbounded_channel();
        let task = tokio::spawn(run_channel(self.inner.clone(), channel.clone(), tx, orx));
        self.inner
            .channels
            .lock()
            .unwrap()
            .insert(channel.id.clone(), ChannelHandle { task, outbox: otx });
        Ok(())
    }

    async fn disconnect(&self, channel: &Channel) -> Result<()> {
        if let Some(handle) = self.inner.channels.lock().unwrap().remove(&channel.id) {
            handle.task.abort();
        }
        Ok(())
    }

    async fn send_message(&self, channel: &Channel, text: &str) -> Result<()> {
        self.inner.auth(false).await?;
        let channels = self.inner.channels.lock().unwrap();
        let handle = channels
            .get(&channel.id)
            .ok_or_else(|| Error::Other(format!("chat {} non connecté", channel.display_name)))?;
        let clean = text.replace(['\r', '\n'], " ");
        handle
            .outbox
            .send(clean)
            .map_err(|_| Error::Other("connexion au chat interrompue".into()))
    }

    async fn moderate(&self, action: &ModerationAction) -> Result<()> {
        let inner = &self.inner;
        let me = inner.auth(false).await?.user_id;
        let broadcaster = inner.user_id(&action.channel.external_id).await?;
        let scope = [
            ("broadcaster_id", broadcaster.as_str()),
            ("moderator_id", me.as_str()),
        ];
        match &action.kind {
            ModerationKind::DeleteMessage { message_id } => {
                let q = [scope[0], scope[1], ("message_id", message_id.as_str())];
                inner
                    .helix(Method::DELETE, "moderation/chat", &q, None)
                    .await?;
            }
            ModerationKind::Timeout { user_id, seconds } => {
                let body = json!({"data": {"user_id": user_id, "duration": seconds, "reason": DEFAULT_REASON}});
                inner
                    .helix(Method::POST, "moderation/bans", &scope, Some(body))
                    .await?;
            }
            ModerationKind::Ban { user_id, reason } => {
                let body = json!({"data": {"user_id": user_id, "reason": reason.as_deref().unwrap_or(DEFAULT_REASON)}});
                inner
                    .helix(Method::POST, "moderation/bans", &scope, Some(body))
                    .await?;
            }
            ModerationKind::Unban { user_id } => {
                let q = [scope[0], scope[1], ("user_id", user_id.as_str())];
                inner
                    .helix(Method::DELETE, "moderation/bans", &q, None)
                    .await?;
            }
        }
        Ok(())
    }

    async fn stream_info(&self, channel: &Channel) -> Result<Option<StreamInfo>> {
        if self.inner.creds.lock().await.is_none() {
            return Ok(None);
        }
        let v = self
            .inner
            .helix(
                Method::GET,
                "streams",
                &[("user_login", &channel.external_id)],
                None,
            )
            .await?;
        let Some(s) = v["data"].get(0) else {
            return Ok(None);
        };
        Ok(Some(StreamInfo {
            channel_id: channel.id.clone(),
            title: s["title"].as_str().unwrap_or_default().to_string(),
            viewers: s["viewer_count"].as_u64().unwrap_or(0) as u32,
            started_at: s["started_at"].as_str().unwrap_or_default().to_string(),
        }))
    }
}

/// Maintient la connexion IRC d'une chaîne, avec reconnexion à délai croissant.
async fn run_channel(
    inner: Arc<Inner>,
    channel: Channel,
    tx: mpsc::Sender<ChatEvent>,
    mut outbox: mpsc::UnboundedReceiver<String>,
) {
    let mut backoff = 1;
    loop {
        let started = Instant::now();
        let _ = session(&inner, &channel, &tx, &mut outbox).await;
        if tx
            .send(ChatEvent::Disconnected {
                channel_id: channel.id.clone(),
            })
            .await
            .is_err()
        {
            return;
        }
        if started.elapsed() > Duration::from_secs(30) {
            backoff = 1;
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn session(
    inner: &Inner,
    channel: &Channel,
    tx: &mpsc::Sender<ChatEvent>,
    outbox: &mut mpsc::UnboundedReceiver<String>,
) -> Result<()> {
    let net = |e: tokio_tungstenite::tungstenite::Error| Error::Network(e.to_string());
    let (mut ws, _) = tokio_tungstenite::connect_async(IRC_URL)
        .await
        .map_err(net)?;
    let login = channel.external_id.to_lowercase();

    // Sans compte connecté, on lit le chat en anonyme (pseudo `justinfan*`).
    let (nick, pass) = match inner.auth(false).await {
        Ok(c) => (c.login, format!("oauth:{}", c.tokens.access_token)),
        Err(_) => (
            format!("justinfan{}", now_secs() % 100_000),
            "SCHMOOPIIE".to_string(),
        ),
    };
    for line in [
        "CAP REQ :twitch.tv/tags twitch.tv/commands".to_string(),
        format!("PASS {pass}"),
        format!("NICK {nick}"),
        format!("JOIN #{login}"),
    ] {
        ws.send(Message::Text(line)).await.map_err(net)?;
    }

    loop {
        tokio::select! {
            incoming = ws.next() => {
                let text = match incoming {
                    Some(Ok(Message::Text(t))) => t,
                    Some(Ok(Message::Close(_))) | None => return Ok(()),
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => return Err(net(e)),
                };
                for raw in text.lines() {
                    let Some(line) = irc::parse_line(raw) else { continue };
                    match line.command {
                        "PING" => ws
                            .send(Message::Text(format!("PONG :{}", line.trailing.unwrap_or("tmi.twitch.tv"))))
                            .await
                            .map_err(net)?,
                        "RECONNECT" => return Ok(()),
                        "366" => {
                            let _ = tx.send(ChatEvent::Connected { channel_id: channel.id.clone() }).await;
                        }
                        "NOTICE" if line.trailing.is_some_and(|t| t.contains("authentication failed")) => {
                            return Err(Error::NotAuthenticated("Twitch"));
                        }
                        _ => {
                            if let Some(event) = irc::to_event(&line, channel) {
                                if tx.send(event).await.is_err() {
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }
            out = outbox.recv() => {
                let Some(text) = out else { return Ok(()) };
                ws.send(Message::Text(format!("PRIVMSG #{login} :{text}"))).await.map_err(net)?;
            }
        }
    }
}
