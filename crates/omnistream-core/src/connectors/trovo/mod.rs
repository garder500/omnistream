//! Connecteur Trovo : chat en lecture via le websocket officiel (jeton obtenu par
//! `/chat/channel-token`), envoi de messages et modération de base via l'API Open Platform.

mod chat;

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

use super::{ChatConnector, ChatEvent, TokenHook};
use crate::auth::trovo::API_URL;
use crate::auth::{self, now_secs, TokenSet};
use crate::domain::{Channel, ModerationAction, ModerationKind, Platform, StreamInfo};
use crate::{Error, Result};

const CHAT_URL: &str = "wss://open-chat.trovo.live/chat";
/// Au-delà, on oublie les expéditeurs mémorisés (nécessaires à la suppression de messages).
const MAX_SENDERS: usize = 5000;

/// Application Trovo (Client ID + secret), requise même pour lire le chat.
#[derive(Clone)]
pub struct TrovoApp {
    pub client_id: String,
    pub client_secret: String,
}

/// Compte connecté, nécessaire pour écrire et modérer.
#[derive(Clone)]
pub struct TrovoAccount {
    pub user_id: String,
    pub login: String,
    pub tokens: TokenSet,
}

#[derive(Default)]
struct Config {
    app: Option<TrovoApp>,
    account: Option<TrovoAccount>,
}

struct ChannelHandle {
    task: JoinHandle<()>,
}

struct Inner {
    http: reqwest::Client,
    config: Mutex<Config>,
    channels: StdMutex<HashMap<String, ChannelHandle>>,
    /// login de chaîne -> channel_id Trovo.
    channel_ids: StdMutex<HashMap<String, String>>,
    /// message_id -> id utilisateur de l'expéditeur.
    senders: StdMutex<HashMap<String, String>>,
    hook: StdMutex<Option<TokenHook>>,
}

#[derive(Clone)]
pub struct TrovoConnector {
    inner: Arc<Inner>,
}

impl Default for TrovoConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl TrovoConnector {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                http: reqwest::Client::new(),
                config: Mutex::new(Config::default()),
                channels: StdMutex::default(),
                channel_ids: StdMutex::default(),
                senders: StdMutex::default(),
                hook: StdMutex::default(),
            }),
        }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.inner.http
    }

    pub async fn set_app(&self, app: Option<TrovoApp>) {
        self.inner.config.lock().await.app = app;
    }

    pub async fn set_account(&self, account: Option<TrovoAccount>) {
        self.inner.config.lock().await.account = account;
    }

    pub fn on_token_refresh(&self, hook: TokenHook) {
        *self.inner.hook.lock().unwrap() = Some(hook);
    }
}

/// Nom d'utilisateur sûr à insérer dans une commande de chat.
fn command_target(user: &str) -> Result<&str> {
    let user = user.trim().trim_start_matches('@');
    if user.is_empty() || !user.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(Error::Other("pseudo Trovo invalide".into()));
    }
    Ok(user)
}

fn moderation_command(kind: &ModerationKind) -> Option<Result<String>> {
    Some(match kind {
        ModerationKind::DeleteMessage { .. } => return None,
        ModerationKind::Timeout { user_id, seconds } => {
            command_target(user_id).map(|u| format!("/timeout {u} {seconds}"))
        }
        ModerationKind::Ban { user_id, .. } => command_target(user_id).map(|u| format!("/ban {u}")),
        ModerationKind::Unban { user_id } => command_target(user_id).map(|u| format!("/unban {u}")),
    })
}

fn parse_stream_info(v: &Value, channel_id: &str) -> Option<StreamInfo> {
    if !v["is_live"].as_bool().unwrap_or(false) {
        return None;
    }
    Some(StreamInfo {
        channel_id: channel_id.to_string(),
        title: v["live_title"].as_str().unwrap_or_default().to_string(),
        viewers: v["current_viewers"].as_u64().unwrap_or(0) as u32,
        started_at: value_text(&v["started_at"]).unwrap_or_default(),
    })
}

fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

impl Inner {
    async fn app(&self) -> Result<TrovoApp> {
        self.config
            .lock()
            .await
            .app
            .clone()
            .ok_or(Error::NotAuthenticated("Trovo"))
    }

    /// Compte valide (jeton renouvelé s'il expire dans moins d'une minute).
    async fn account(&self, force_refresh: bool) -> Result<(TrovoApp, TrovoAccount)> {
        let mut cfg = self.config.lock().await;
        let app = cfg.app.clone().ok_or(Error::NotAuthenticated("Trovo"))?;
        let acc = cfg
            .account
            .as_mut()
            .ok_or(Error::NotAuthenticated("Trovo"))?;
        let expiring = acc.tokens.expires_at.is_some_and(|e| e - 60 < now_secs());
        if force_refresh || expiring {
            if let Some(refresh) = acc.tokens.refresh_token.clone() {
                match auth::trovo::refresh(&self.http, &app.client_id, &app.client_secret, &refresh)
                    .await
                {
                    Ok(mut fresh) => {
                        // Trovo peut ne pas renvoyer de nouveau refresh token : on garde l'ancien.
                        fresh.refresh_token = fresh.refresh_token.or(Some(refresh));
                        acc.tokens = fresh;
                        if let Some(hook) = self.hook.lock().unwrap().clone() {
                            hook(&acc.user_id, &acc.tokens);
                        }
                    }
                    // Jeton encore valide : on tente quand même la requête.
                    Err(_)
                        if !force_refresh
                            && !acc.tokens.expires_at.is_some_and(|e| e < now_secs()) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        Ok((app, acc.clone()))
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        as_user: bool,
    ) -> Result<Value> {
        let mut attempt = 0;
        loop {
            let mut req = self
                .http
                .request(method.clone(), format!("{API_URL}/{path}"));
            if as_user {
                let (app, acc) = self.account(attempt > 0).await?;
                req = req.header("client-id", app.client_id).header(
                    "Authorization",
                    format!("OAuth {}", acc.tokens.access_token),
                );
            } else {
                req = req.header("client-id", self.app().await?.client_id);
            }
            if let Some(b) = &body {
                req = req.json(b);
            }
            let resp = req.send().await?;
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED && as_user && attempt == 0 {
                attempt += 1;
                continue;
            }
            let text = resp.text().await?;
            let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if !status.is_success() {
                let msg = value["message"].as_str().unwrap_or(text.as_str());
                return Err(Error::Other(format!("Trovo ({status}): {msg}")));
            }
            // Trovo peut répondre 200 avec un champ `status`/`message` d'erreur.
            if let Some(msg) = value["error"].as_str().filter(|e| !e.is_empty()) {
                return Err(Error::Other(format!("Trovo: {msg}")));
            }
            return Ok(value);
        }
    }

    async fn channel_id(&self, login: &str) -> Result<String> {
        let login = login.to_lowercase();
        if let Some(id) = self.channel_ids.lock().unwrap().get(&login) {
            return Ok(id.clone());
        }
        let v = self
            .call(
                Method::POST,
                "channels/id",
                Some(json!({ "username": login })),
                false,
            )
            .await?;
        let id = value_text(&v["channel_id"])
            .or_else(|| value_text(&v["channelId"]))
            .ok_or_else(|| Error::Other(format!("chaîne Trovo introuvable: {login}")))?;
        self.channel_ids.lock().unwrap().insert(login, id.clone());
        Ok(id)
    }

    async fn chat_token(&self, channel_id: &str) -> Result<String> {
        let v = self
            .call(
                Method::GET,
                &format!("chat/channel-token/{channel_id}"),
                None,
                false,
            )
            .await?;
        value_text(&v["token"])
            .ok_or_else(|| Error::Other("Trovo n'a pas fourni de jeton de chat".into()))
    }

    async fn say(&self, channel_id: &str, content: &str) -> Result<()> {
        self.call(
            Method::POST,
            "chat/send",
            Some(json!({ "content": content, "channel_id": channel_id })),
            true,
        )
        .await
        .map(|_| ())
    }

    fn remember_sender(&self, message_id: String, sender_id: String) {
        let mut senders = self.senders.lock().unwrap();
        if senders.len() >= MAX_SENDERS {
            senders.clear();
        }
        senders.insert(message_id, sender_id);
    }
}

#[async_trait]
impl ChatConnector for TrovoConnector {
    fn platform(&self) -> Platform {
        Platform::Trovo
    }

    async fn connect(&self, channel: &Channel, tx: mpsc::Sender<ChatEvent>) -> Result<()> {
        self.disconnect(channel).await?;
        let task = tokio::spawn(run_channel(self.inner.clone(), channel.clone(), tx));
        self.inner
            .channels
            .lock()
            .unwrap()
            .insert(channel.id.clone(), ChannelHandle { task });
        Ok(())
    }

    async fn disconnect(&self, channel: &Channel) -> Result<()> {
        if let Some(handle) = self.inner.channels.lock().unwrap().remove(&channel.id) {
            handle.task.abort();
        }
        Ok(())
    }

    async fn send_message(&self, channel: &Channel, text: &str) -> Result<()> {
        let channel_id = self.inner.channel_id(&channel.external_id).await?;
        let clean = text.replace(['\r', '\n'], " ");
        self.inner.say(&channel_id, &clean).await
    }

    async fn stream_info(&self, channel: &Channel) -> Result<Option<StreamInfo>> {
        if self.inner.app().await.is_err() {
            return Ok(None);
        }
        let v = self
            .inner
            .call(
                Method::POST,
                "channels/id",
                Some(json!({ "username": channel.external_id.to_lowercase() })),
                false,
            )
            .await?;
        Ok(parse_stream_info(&v, &channel.id))
    }

    async fn moderate(&self, action: &ModerationAction) -> Result<()> {
        let inner = &self.inner;
        let channel_id = inner.channel_id(&action.channel.external_id).await?;
        if let ModerationKind::DeleteMessage { message_id } = &action.kind {
            let uid = inner
                .senders
                .lock()
                .unwrap()
                .get(message_id)
                .cloned()
                .ok_or_else(|| {
                    Error::Other("auteur du message Trovo inconnu (message trop ancien)".into())
                })?;
            let path = format!("channels/{channel_id}/messages/{message_id}/users/{uid}");
            inner.call(Method::DELETE, &path, None, true).await?;
            return Ok(());
        }
        // Les sanctions passent par les commandes de chat, qui s'exécutent avec les droits du compte connecté.
        let command = moderation_command(&action.kind).expect("DeleteMessage traité plus haut")?;
        inner.say(&channel_id, &command).await
    }
}

/// Maintient la connexion websocket d'une chaîne, avec reconnexion à délai croissant.
async fn run_channel(inner: Arc<Inner>, channel: Channel, tx: mpsc::Sender<ChatEvent>) {
    let mut backoff = 1;
    loop {
        let started = Instant::now();
        if let Err(e) = session(&inner, &channel, &tx).await {
            eprintln!("chat Trovo {}: {e}", channel.display_name);
        }
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

async fn session(inner: &Inner, channel: &Channel, tx: &mpsc::Sender<ChatEvent>) -> Result<()> {
    let net = |e: tokio_tungstenite::tungstenite::Error| Error::Network(e.to_string());
    let channel_id = inner.channel_id(&channel.external_id).await?;
    let token = inner.chat_token(&channel_id).await?;
    let (mut ws, _) = tokio_tungstenite::connect_async(CHAT_URL)
        .await
        .map_err(net)?;
    let auth = json!({"type": "AUTH", "nonce": "auth", "data": {"token": token}});
    ws.send(Message::Text(auth.to_string()))
        .await
        .map_err(net)?;

    let mut gap = Duration::from_secs(30);
    let mut ping = tokio::time::interval(gap);
    ping.reset();
    let mut counter = 0u64;
    loop {
        tokio::select! {
            incoming = ws.next() => {
                let text = match incoming {
                    Some(Ok(Message::Text(t))) => t,
                    Some(Ok(Message::Close(_))) | None => return Ok(()),
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => return Err(net(e)),
                };
                match chat::parse_frame(&text, channel) {
                    chat::Frame::Response { nonce, error } if nonce == "auth" => match error {
                        None => {
                            let _ = tx.send(ChatEvent::Connected { channel_id: channel.id.clone() }).await;
                        }
                        Some(e) => return Err(Error::Other(format!("authentification au chat Trovo refusée: {e}"))),
                    },
                    chat::Frame::Pong { gap: secs } => {
                        if Duration::from_secs(secs) != gap {
                            gap = Duration::from_secs(secs);
                            ping = tokio::time::interval(gap);
                            ping.reset();
                        }
                    }
                    chat::Frame::Chat(chats) => {
                        for parsed in chats {
                            if let Some((message_id, sender_id)) = parsed.sender {
                                inner.remember_sender(message_id, sender_id);
                            }
                            if tx.send(parsed.event).await.is_err() {
                                return Ok(());
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ = ping.tick() => {
                counter += 1;
                let frame = json!({"type": "PING", "nonce": format!("ping{counter}")});
                ws.send(Message::Text(frame.to_string())).await.map_err(net)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_moderation_commands() {
        let cmd = |k: ModerationKind| moderation_command(&k).unwrap().unwrap();
        assert_eq!(
            cmd(ModerationKind::Ban {
                user_id: "@Fan_42".into(),
                reason: None
            }),
            "/ban Fan_42"
        );
        assert_eq!(
            cmd(ModerationKind::Timeout {
                user_id: "fan".into(),
                seconds: 600
            }),
            "/timeout fan 600"
        );
        assert_eq!(
            cmd(ModerationKind::Unban {
                user_id: "fan".into()
            }),
            "/unban fan"
        );
        assert!(moderation_command(&ModerationKind::DeleteMessage {
            message_id: "m".into()
        })
        .is_none());
    }

    #[test]
    fn rejects_injection_in_commands() {
        let r = moderation_command(&ModerationKind::Ban {
            user_id: "fan\n/mod x".into(),
            reason: None,
        })
        .unwrap();
        assert!(r.is_err());
        assert!(command_target("").is_err());
    }

    #[test]
    fn parses_stream_info() {
        let live = json!({"is_live": true, "live_title": "Soirée", "current_viewers": 12, "started_at": "2026-01-01T10:00:00Z"});
        let info = parse_stream_info(&live, "trovo:g").unwrap();
        assert_eq!((info.title.as_str(), info.viewers), ("Soirée", 12));
        assert!(parse_stream_info(&json!({"is_live": false}), "trovo:g").is_none());
    }

    #[test]
    fn extracts_text_ids() {
        assert_eq!(value_text(&json!(123)).as_deref(), Some("123"));
        assert_eq!(value_text(&json!("abc")).as_deref(), Some("abc"));
        assert_eq!(value_text(&json!(null)), None);
    }
}
