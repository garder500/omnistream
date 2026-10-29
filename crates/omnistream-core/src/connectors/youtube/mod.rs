//! Connecteur YouTube : chat du direct en lecture par sondage de `liveChatMessages.list`,
//! envoi et modération via l'API YouTube Data v3 (OAuth Google, quota de 10 000 unités par jour).

mod chat;

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::Method;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use super::{ChatConnector, ChatEvent, TokenHook};
use crate::auth::youtube::API_URL;
use crate::auth::{self, now_secs, TokenSet};
use crate::domain::{Channel, ModerationAction, ModerationKind, Platform, StreamInfo};
use crate::{Error, Result};

/// Limite de longueur d'un message de chat YouTube.
const MAX_MESSAGE_CHARS: usize = 200;
/// Attente avant de rechercher à nouveau le direct d'une chaîne hors ligne. Les chaînes
/// tierces passent par `search.list` (100 unités de quota), d'où un délai plus long.
const OFFLINE_WAIT_OWN: Duration = Duration::from_secs(30);
const OFFLINE_WAIT_OTHER: Duration = Duration::from_secs(300);

/// Identifiants OAuth Google (type « Application de bureau »).
#[derive(Clone)]
pub struct YoutubeApp {
    pub client_id: String,
    pub client_secret: String,
}

/// Compte connecté : nécessaire à toute requête, y compris la lecture du chat.
#[derive(Clone)]
pub struct YoutubeAccount {
    pub user_id: String,
    pub channel_id: String,
    pub tokens: TokenSet,
}

#[derive(Default)]
struct Config {
    app: Option<YoutubeApp>,
    account: Option<YoutubeAccount>,
}

struct ChannelHandle {
    task: JoinHandle<()>,
}

struct Inner {
    http: reqwest::Client,
    config: Mutex<Config>,
    channels: StdMutex<HashMap<String, ChannelHandle>>,
    /// identifiant de chaîne Omnistream -> `liveChatId` du direct en cours.
    chat_ids: StdMutex<HashMap<String, String>>,
    /// handle -> identifiant de chaîne YouTube (`UC...`).
    channel_ids: StdMutex<HashMap<String, String>>,
    /// `liveChatId:utilisateur` -> identifiant du bannissement, nécessaire pour le lever.
    bans: StdMutex<HashMap<String, String>>,
    hook: StdMutex<Option<TokenHook>>,
}

#[derive(Clone)]
pub struct YoutubeConnector {
    inner: Arc<Inner>,
}

impl Default for YoutubeConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl YoutubeConnector {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                http: reqwest::Client::new(),
                config: Mutex::new(Config::default()),
                channels: StdMutex::default(),
                chat_ids: StdMutex::default(),
                channel_ids: StdMutex::default(),
                bans: StdMutex::default(),
                hook: StdMutex::default(),
            }),
        }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.inner.http
    }

    pub async fn set_app(&self, app: Option<YoutubeApp>) {
        self.inner.config.lock().await.app = app;
    }

    pub async fn set_account(&self, account: Option<YoutubeAccount>) {
        self.inner.config.lock().await.account = account;
    }

    pub fn on_token_refresh(&self, hook: TokenHook) {
        *self.inner.hook.lock().unwrap() = Some(hook);
    }
}

/// Direct en cours d'une chaîne.
#[derive(Debug, Clone, PartialEq)]
struct Live {
    chat_id: Option<String>,
    title: String,
    viewers: u32,
    started_at: String,
}

fn parse_live(v: &Value) -> Option<Live> {
    let item = v["items"].get(0)?;
    let details = &item["liveStreamingDetails"];
    // Un direct terminé garde ses détails mais n'a plus de `activeLiveChatId`.
    details["actualStartTime"].as_str()?;
    if details["actualEndTime"].is_string() {
        return None;
    }
    Some(Live {
        chat_id: details["activeLiveChatId"].as_str().map(str::to_owned),
        title: item["snippet"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        viewers: details["concurrentViewers"]
            .as_str()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0),
        started_at: details["actualStartTime"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    })
}

/// `true` quand l'erreur signifie que le chat du direct n'existe plus.
fn is_chat_over(e: &Error) -> bool {
    let msg = e.to_string();
    msg.contains("liveChatEnded") || msg.contains("liveChatNotFound")
}

fn is_channel_id(s: &str) -> bool {
    s.len() == 24 && s.starts_with("UC")
}

fn ban_body(chat_id: &str, user_channel_id: &str, seconds: Option<u32>) -> Value {
    let mut snippet = json!({
        "liveChatId": chat_id,
        "type": if seconds.is_some() { "temporary" } else { "permanent" },
        "bannedUserDetails": { "channelId": user_channel_id },
    });
    if let Some(s) = seconds {
        snippet["banDurationSeconds"] = json!(s);
    }
    json!({ "snippet": snippet })
}

impl Inner {
    /// Compte valide (jeton renouvelé s'il expire dans moins d'une minute).
    async fn account(&self, force_refresh: bool) -> Result<YoutubeAccount> {
        let mut cfg = self.config.lock().await;
        let app = cfg.app.clone().ok_or(Error::NotAuthenticated("YouTube"))?;
        let acc = cfg
            .account
            .as_mut()
            .ok_or(Error::NotAuthenticated("YouTube"))?;
        let expiring = acc.tokens.expires_at.is_some_and(|e| e - 60 < now_secs());
        if force_refresh || expiring {
            if let Some(refresh) = acc.tokens.refresh_token.clone() {
                match auth::youtube::refresh(
                    &self.http,
                    &app.client_id,
                    &app.client_secret,
                    &refresh,
                )
                .await
                {
                    Ok(mut fresh) => {
                        // Google ne renvoie pas de nouveau refresh token : on garde l'ancien.
                        fresh.refresh_token = fresh.refresh_token.or(Some(refresh));
                        acc.tokens = fresh;
                        if let Some(hook) = self.hook.lock().unwrap().clone() {
                            hook(&acc.user_id, &acc.tokens);
                        }
                    }
                    Err(_)
                        if !force_refresh
                            && !acc.tokens.expires_at.is_some_and(|e| e < now_secs()) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(acc.clone())
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> Result<Value> {
        let mut attempt = 0;
        loop {
            let acc = self.account(attempt > 0).await?;
            let mut req = self
                .http
                .request(method.clone(), format!("{API_URL}/{path}"))
                .query(query)
                .bearer_auth(&acc.tokens.access_token);
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
                let reason = value["error"]["errors"][0]["reason"].as_str().unwrap_or("");
                let msg = value["error"]["message"].as_str().unwrap_or(text.as_str());
                return Err(Error::Other(format!("YouTube ({status} {reason}): {msg}")));
            }
            return Ok(value);
        }
    }

    async fn own_channel_id(&self) -> Result<String> {
        Ok(self.account(false).await?.channel_id)
    }

    /// Identifiant `UC...` d'une chaîne désignée par son handle (avec ou sans `@`) ou son id.
    async fn channel_id(&self, external_id: &str) -> Result<String> {
        let name = external_id.trim().trim_start_matches('@');
        if is_channel_id(name) {
            return Ok(name.to_string());
        }
        let key = name.to_lowercase();
        if let Some(id) = self.channel_ids.lock().unwrap().get(&key) {
            return Ok(id.clone());
        }
        let handle = format!("@{name}");
        let v = self
            .call(
                Method::GET,
                "channels",
                &[("part", "id"), ("forHandle", handle.as_str())],
                None,
            )
            .await?;
        let id = v["items"][0]["id"]
            .as_str()
            .ok_or_else(|| Error::Other(format!("chaîne YouTube introuvable: {name}")))?
            .to_string();
        self.channel_ids.lock().unwrap().insert(key, id.clone());
        Ok(id)
    }

    /// Direct en cours de `channel_id` ; `None` si la chaîne n'est pas en direct.
    async fn find_live(&self, channel_id: &str) -> Result<Option<Live>> {
        let own = self.own_channel_id().await? == channel_id;
        let found = if own {
            self.call(
                Method::GET,
                "liveBroadcasts",
                &[
                    ("part", "id"),
                    ("broadcastStatus", "active"),
                    ("mine", "true"),
                ],
                None,
            )
            .await?
        } else {
            self.call(
                Method::GET,
                "search",
                &[
                    ("part", "id"),
                    ("channelId", channel_id),
                    ("eventType", "live"),
                    ("type", "video"),
                    ("maxResults", "1"),
                ],
                None,
            )
            .await?
        };
        let item = &found["items"][0];
        let Some(video_id) = (if own {
            item["id"].as_str()
        } else {
            item["id"]["videoId"].as_str()
        }) else {
            return Ok(None);
        };
        let v = self
            .call(
                Method::GET,
                "videos",
                &[("part", "snippet,liveStreamingDetails"), ("id", video_id)],
                None,
            )
            .await?;
        Ok(parse_live(&v))
    }

    /// `liveChatId` du direct de `channel`, depuis le sondage en cours ou une nouvelle recherche.
    async fn chat_id(&self, channel: &Channel) -> Result<String> {
        if let Some(id) = self.chat_ids.lock().unwrap().get(&channel.id) {
            return Ok(id.clone());
        }
        let channel_id = self.channel_id(&channel.external_id).await?;
        self.find_live(&channel_id)
            .await?
            .and_then(|l| l.chat_id)
            .ok_or_else(|| Error::Other("la chaîne YouTube n'est pas en direct".into()))
    }
}

#[async_trait]
impl ChatConnector for YoutubeConnector {
    fn platform(&self) -> Platform {
        Platform::Youtube
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
        self.inner.chat_ids.lock().unwrap().remove(&channel.id);
        Ok(())
    }

    async fn send_message(&self, channel: &Channel, text: &str) -> Result<()> {
        let chat_id = self.inner.chat_id(channel).await?;
        let clean: String = text
            .replace(['\r', '\n'], " ")
            .chars()
            .take(MAX_MESSAGE_CHARS)
            .collect();
        let body = json!({"snippet": {
            "liveChatId": chat_id,
            "type": "textMessageEvent",
            "textMessageDetails": {"messageText": clean},
        }});
        self.inner
            .call(
                Method::POST,
                "liveChat/messages",
                &[("part", "snippet")],
                Some(body),
            )
            .await
            .map(|_| ())
    }

    async fn stream_info(&self, channel: &Channel) -> Result<Option<StreamInfo>> {
        if self.inner.account(false).await.is_err() {
            return Ok(None);
        }
        let channel_id = self.inner.channel_id(&channel.external_id).await?;
        Ok(self
            .inner
            .find_live(&channel_id)
            .await?
            .map(|l| StreamInfo {
                channel_id: channel.id.clone(),
                title: l.title,
                viewers: l.viewers,
                started_at: l.started_at,
            }))
    }

    async fn moderate(&self, action: &ModerationAction) -> Result<()> {
        let inner = &self.inner;
        let chat_id = inner.chat_id(&action.channel).await?;
        let ban_key = |user: &str| format!("{chat_id}:{user}");
        match &action.kind {
            ModerationKind::DeleteMessage { message_id } => inner
                .call(
                    Method::DELETE,
                    "liveChat/messages",
                    &[("id", message_id.as_str())],
                    None,
                )
                .await
                .map(|_| ()),
            ModerationKind::Timeout { user_id, seconds } => inner
                .call(
                    Method::POST,
                    "liveChat/bans",
                    &[("part", "snippet")],
                    Some(ban_body(&chat_id, user_id, Some(*seconds))),
                )
                .await
                .map(|v| remember_ban(inner, ban_key(user_id), &v)),
            ModerationKind::Ban { user_id, .. } => inner
                .call(
                    Method::POST,
                    "liveChat/bans",
                    &[("part", "snippet")],
                    Some(ban_body(&chat_id, user_id, None)),
                )
                .await
                .map(|v| remember_ban(inner, ban_key(user_id), &v)),
            ModerationKind::Unban { user_id } => {
                let ban_id = inner
                    .bans
                    .lock()
                    .unwrap()
                    .remove(&ban_key(user_id))
                    .ok_or_else(|| {
                        Error::Other(
                            "bannissement YouTube inconnu : seuls les bans faits depuis Omnistream pendant ce direct peuvent être levés".into(),
                        )
                    })?;
                inner
                    .call(
                        Method::DELETE,
                        "liveChat/bans",
                        &[("id", ban_id.as_str())],
                        None,
                    )
                    .await
                    .map(|_| ())
            }
        }
    }
}

fn remember_ban(inner: &Inner, key: String, response: &Value) {
    if let Some(id) = response["id"].as_str() {
        inner.bans.lock().unwrap().insert(key, id.to_string());
    }
}

enum Ended {
    /// Pas de direct (ou direct terminé) : on cherchera à nouveau plus tard.
    Offline,
}

/// Maintient le sondage du chat d'une chaîne, avec reprise automatique.
async fn run_channel(inner: Arc<Inner>, channel: Channel, tx: mpsc::Sender<ChatEvent>) {
    let mut backoff = 2;
    loop {
        let started = Instant::now();
        let mut connected = false;
        let outcome = session(&inner, &channel, &tx, &mut connected).await;
        inner.chat_ids.lock().unwrap().remove(&channel.id);
        if connected
            && tx
                .send(ChatEvent::Disconnected {
                    channel_id: channel.id.clone(),
                })
                .await
                .is_err()
        {
            return;
        }
        let wait = match outcome {
            Ok(Ended::Offline) => {
                let own = match inner.channel_id(&channel.external_id).await {
                    Ok(id) => inner.own_channel_id().await.is_ok_and(|me| me == id),
                    Err(_) => false,
                };
                if own {
                    OFFLINE_WAIT_OWN
                } else {
                    OFFLINE_WAIT_OTHER
                }
            }
            Err(e) => {
                eprintln!("chat YouTube {}: {e}", channel.display_name);
                if started.elapsed() > Duration::from_secs(60) {
                    backoff = 2;
                }
                let wait = Duration::from_secs(backoff);
                backoff = (backoff * 2).min(60);
                wait
            }
        };
        tokio::time::sleep(wait).await;
    }
}

async fn session(
    inner: &Inner,
    channel: &Channel,
    tx: &mpsc::Sender<ChatEvent>,
    connected: &mut bool,
) -> Result<Ended> {
    let channel_id = inner.channel_id(&channel.external_id).await?;
    let Some(live) = inner.find_live(&channel_id).await? else {
        return Ok(Ended::Offline);
    };
    let Some(chat_id) = live.chat_id else {
        return Ok(Ended::Offline);
    };
    inner
        .chat_ids
        .lock()
        .unwrap()
        .insert(channel.id.clone(), chat_id.clone());

    let mut page_token: Option<String> = None;
    loop {
        let mut query = vec![
            ("liveChatId", chat_id.as_str()),
            ("part", "snippet,authorDetails"),
            ("maxResults", "200"),
        ];
        if let Some(t) = &page_token {
            query.push(("pageToken", t.as_str()));
        }
        let value = match inner
            .call(Method::GET, "liveChat/messages", &query, None)
            .await
        {
            Ok(v) => v,
            Err(e) if is_chat_over(&e) => return Ok(Ended::Offline),
            Err(e) => return Err(e),
        };
        let page = chat::parse_page(&value, channel);
        if page_token.is_none() {
            // La première page est l'historique récent : on ne le rejoue pas à chaque reconnexion.
            *connected = true;
            let _ = tx
                .send(ChatEvent::Connected {
                    channel_id: channel.id.clone(),
                })
                .await;
        } else {
            for event in page.events {
                if tx.send(event).await.is_err() {
                    return Ok(Ended::Offline);
                }
            }
        }
        page_token = Some(
            page.next_page_token
                .ok_or_else(|| Error::Other("YouTube n'a pas renvoyé de jeton de page".into()))?,
        );
        tokio::time::sleep(Duration::from_millis(page.poll_ms)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_live_details() {
        let v = json!({"items":[{"snippet":{"title":"Soirée"},"liveStreamingDetails":{
            "actualStartTime":"2026-01-01T10:00:00Z","concurrentViewers":"42","activeLiveChatId":"chat1"}}]});
        let live = parse_live(&v).unwrap();
        assert_eq!(
            (live.chat_id.as_deref(), live.title.as_str(), live.viewers),
            (Some("chat1"), "Soirée", 42)
        );
        let ended = json!({"items":[{"liveStreamingDetails":{
            "actualStartTime":"2026-01-01T10:00:00Z","actualEndTime":"2026-01-01T12:00:00Z"}}]});
        assert!(parse_live(&ended).is_none());
        let upcoming = json!({"items":[{"liveStreamingDetails":{"scheduledStartTime":"2026-02-01T10:00:00Z"}}]});
        assert!(parse_live(&upcoming).is_none());
        assert!(parse_live(&json!({"items":[]})).is_none());
    }

    #[test]
    fn detects_finished_chat() {
        let over = Error::Other("YouTube (403 Forbidden liveChatEnded): fini".into());
        assert!(is_chat_over(&over));
        assert!(!is_chat_over(&Error::Other(
            "YouTube (403 quotaExceeded): quota".into()
        )));
    }

    #[test]
    fn recognizes_channel_ids() {
        assert!(is_channel_id("UCX6OQ3DkcsbYNE6H8uQQuVA"));
        assert!(!is_channel_id("garder500"));
        assert!(!is_channel_id("UCshort"));
    }

    #[test]
    fn builds_ban_bodies() {
        let perm = ban_body("c", "UCu", None);
        assert_eq!(perm["snippet"]["type"], "permanent");
        assert!(perm["snippet"].get("banDurationSeconds").is_none());
        let temp = ban_body("c", "UCu", Some(600));
        assert_eq!(temp["snippet"]["type"], "temporary");
        assert_eq!(temp["snippet"]["banDurationSeconds"], 600);
        assert_eq!(temp["snippet"]["bannedUserDetails"]["channelId"], "UCu");
    }
}
