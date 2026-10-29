//! Connecteur TikTok en lecture seule (non officiel). TikTok n'a pas d'API de chat publique :
//! la connexion au flux « webcast » exige une URL signée, obtenue auprès du service tiers
//! Euler Stream avec la clé d'API de l'utilisateur. Le websocket, lui, se connecte directement
//! à TikTok. Les requêtes sont donc partagées entre TikTok et ce service tiers.
//!
//! Protocole (inspiré du projet `tiktok-live-connector`) : `room_id` du pseudo, URL signée,
//! websocket protobuf, `im_enter_room`, heartbeat toutes les 10 s, acquittements des lots.

mod proto;

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use flate2::read::GzDecoder;
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use reqwest::StatusCode;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use super::{ChatConnector, ChatEvent};
use crate::auth::loopback::encode;
use crate::domain::{
    Channel, ChatAuthor, ChatMessage, ModerationAction, Platform, StreamEvent, StreamEventKind,
};
use crate::{Error, Result};

const SIGN_URL: &str = "https://api.eulerstream.com";
/// Identifie Omnistream auprès du service de signature.
const CLIENT_NAME: &str = "omnistream";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36 Edg/128.0.2739.79";
const HEARTBEAT: Duration = Duration::from_secs(10);
/// Attente avant de revérifier si une chaîne hors ligne est passée en direct.
const OFFLINE_WAIT: Duration = Duration::from_secs(120);
/// Action de `WebcastControlMessage` indiquant la fin du direct.
const CONTROL_STREAM_ENDED: i32 = 3;
/// Type de cadeau « série » : TikTok en émet un message par palier, le dernier porte `repeat_end`.
const GIFT_TYPE_STREAK: i32 = 1;

struct ChannelHandle {
    task: JoinHandle<()>,
}

struct Inner {
    http: reqwest::Client,
    api_key: Mutex<Option<String>>,
    channels: StdMutex<HashMap<String, ChannelHandle>>,
}

#[derive(Clone)]
pub struct TiktokConnector {
    inner: Arc<Inner>,
}

impl Default for TiktokConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl TiktokConnector {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                http: reqwest::Client::new(),
                api_key: Mutex::new(None),
                channels: StdMutex::default(),
            }),
        }
    }

    /// Clé d'API Euler Stream ; sans elle, aucune chaîne ne peut se connecter.
    pub async fn set_api_key(&self, key: Option<String>) {
        *self.inner.api_key.lock().await = key.filter(|k| !k.trim().is_empty());
    }
}

/// Résultat de l'analyse d'un message du flux.
#[derive(Debug, PartialEq)]
enum Parsed {
    Event(Box<ChatEvent>),
    StreamEnded,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn author_handle(user: &proto::User) -> (String, String) {
    let handle = if user.display_id.is_empty() {
        user.id.to_string()
    } else {
        user.display_id.clone()
    };
    let name = if user.nickname.is_empty() {
        handle.clone()
    } else {
        user.nickname.clone()
    };
    (handle, name)
}

fn message_id(method: &str, common: &proto::Common, user: &proto::User) -> String {
    if common.msg_id != 0 {
        common.msg_id.to_string()
    } else {
        format!("{method}-{}-{}", user.id, common.create_time)
    }
}

fn timestamp_ms(common: &proto::Common) -> i64 {
    if common.create_time > 0 {
        // TikTok renvoie des millisecondes ; on tolère des secondes.
        if common.create_time < 100_000_000_000 {
            common.create_time * 1000
        } else {
            common.create_time
        }
    } else {
        now_ms()
    }
}

/// Convertit un message `webcast` en événement de l'application.
fn parse_message(method: &str, payload: &[u8], channel: &Channel) -> Option<Parsed> {
    let event = |id: String, who: String, kind, message, timestamp_ms| {
        Some(Parsed::Event(Box::new(ChatEvent::Event(StreamEvent {
            id,
            channel_id: channel.id.clone(),
            platform: Platform::Tiktok,
            who,
            kind,
            message,
            timestamp_ms,
        }))))
    };
    match method {
        "WebcastChatMessage" => {
            let m = proto::ChatMessage::decode(payload).ok()?;
            let (common, user) = (m.common.unwrap_or_default(), m.user.unwrap_or_default());
            if m.content.is_empty() {
                return None;
            }
            let identity = m.user_identity.unwrap_or_default();
            let (handle, name) = author_handle(&user);
            Some(Parsed::Event(Box::new(ChatEvent::Message(ChatMessage {
                id: message_id(method, &common, &user),
                channel_id: channel.id.clone(),
                platform: Platform::Tiktok,
                author: ChatAuthor {
                    external_id: handle,
                    display_name: name,
                    is_moderator: identity.is_moderator_of_anchor || identity.is_anchor,
                    is_subscriber: identity.is_subscriber_of_anchor,
                },
                text: m.content,
                timestamp_ms: timestamp_ms(&common),
                emotes: vec![],
                badges: vec![],
                room_id: None,
            }))))
        }
        "WebcastGiftMessage" => {
            let m = proto::GiftMessage::decode(payload).ok()?;
            let gift = m.gift.unwrap_or_default();
            // Une série de cadeaux émet un message par palier : seul le dernier compte.
            if gift.kind == GIFT_TYPE_STREAK && m.repeat_end == 0 {
                return None;
            }
            let (common, user) = (m.common.unwrap_or_default(), m.user.unwrap_or_default());
            let (_, name) = author_handle(&user);
            let label = if gift.name.is_empty() {
                "cadeau".to_string()
            } else {
                gift.name
            };
            event(
                message_id(method, &common, &user),
                name,
                StreamEventKind::Gift { recipient: None },
                Some(format!("{label} x{}", m.repeat_count.max(1))),
                timestamp_ms(&common),
            )
        }
        "WebcastSocialMessage" => {
            let m = proto::SocialMessage::decode(payload).ok()?;
            let (common, user) = (m.common.unwrap_or_default(), m.user.unwrap_or_default());
            let is_follow = common
                .display_text
                .as_ref()
                .is_some_and(|t| t.key.contains("follow"));
            if !is_follow {
                return None;
            }
            let (_, name) = author_handle(&user);
            event(
                message_id(method, &common, &user),
                name,
                StreamEventKind::Follow,
                None,
                timestamp_ms(&common),
            )
        }
        "WebcastSubNotifyMessage" => {
            let m = proto::SubNotifyMessage::decode(payload).ok()?;
            let (common, user) = (m.common.unwrap_or_default(), m.user.unwrap_or_default());
            let (_, name) = author_handle(&user);
            event(
                message_id(method, &common, &user),
                name,
                StreamEventKind::Sub {
                    months: m.sub_month.clamp(1, i64::from(u32::MAX)) as u32,
                },
                None,
                timestamp_ms(&common),
            )
        }
        "WebcastControlMessage" => {
            let m = proto::ControlMessage::decode(payload).ok()?;
            (m.action == CONTROL_STREAM_ENDED).then_some(Parsed::StreamEnded)
        }
        _ => None,
    }
}

fn gunzip_if_needed(payload: Vec<u8>) -> Result<Vec<u8>> {
    if payload.len() > 2 && payload[..3] == [0x1f, 0x8b, 0x08] {
        let mut out = Vec::new();
        GzDecoder::new(payload.as_slice())
            .read_to_end(&mut out)
            .map_err(|e| Error::Other(format!("trame TikTok illisible: {e}")))?;
        Ok(out)
    } else {
        Ok(payload)
    }
}

/// Décode une trame binaire du websocket : lot de messages et trame d'acquittement éventuelle.
fn decode_frame(data: &[u8]) -> Result<(Option<proto::FetchResult>, i64)> {
    let frame = proto::PushFrame::decode(data)
        .map_err(|e| Error::Other(format!("trame TikTok invalide: {e}")))?;
    if frame.payload_type != "msg" || frame.payload_encoding != "pb" || frame.payload.is_empty() {
        return Ok((None, frame.log_id));
    }
    let payload = gunzip_if_needed(frame.payload)?;
    let batch = proto::FetchResult::decode(payload.as_slice())
        .map_err(|e| Error::Other(format!("lot TikTok invalide: {e}")))?;
    Ok((Some(batch), frame.log_id))
}

fn push_frame(payload_type: &str, log_id: i64, payload: Vec<u8>) -> Vec<u8> {
    proto::PushFrame {
        log_id,
        payload_encoding: "pb".into(),
        payload_type: payload_type.into(),
        payload,
        ..Default::default()
    }
    .encode_to_vec()
}

fn enter_room_frame(room_id: i64, unique: i64) -> Vec<u8> {
    let msg = proto::EnterRoom {
        room_id,
        live_id: 12,
        identity: "audience".into(),
        account_type: 0,
        enter_unique_id: unique,
        filter_welcome_msg: "0".into(),
        ..Default::default()
    };
    push_frame("im_enter_room", 0, msg.encode_to_vec())
}

fn heartbeat_frame(room_id: i64, seq: i64) -> Vec<u8> {
    let msg = proto::HeartBeat {
        room_id,
        send_packet_seq_id: seq,
    };
    push_frame("hb", 0, msg.encode_to_vec())
}

fn ack_frame(log_id: i64, internal_ext: &str) -> Vec<u8> {
    push_frame("ack", log_id, internal_ext.as_bytes().to_vec())
}

/// Adresse du websocket : serveur indiqué par TikTok + paramètres de client + paramètres de route.
fn websocket_url(batch: &proto::FetchResult, room_id: &str) -> String {
    let mut params: Vec<(String, String)> = [
        ("version_code", "180800"),
        ("aid", "1988"),
        ("app_language", "en"),
        ("app_name", "tiktok_web"),
        ("browser_platform", "MacIntel"),
        ("browser_language", "en-DE"),
        ("browser_name", "Mozilla"),
        ("browser_version", &USER_AGENT["Mozilla/".len()..]),
        ("browser_online", "true"),
        ("cookie_enabled", "true"),
        ("tz_name", "Europe/Berlin"),
        ("device_platform", "web"),
        ("identity", "audience"),
        ("live_id", "12"),
        ("webcast_language", "en"),
        ("ws_direct", "0"),
        ("sup_ws_ds_opt", "1"),
        ("update_version_code", "2.0.0"),
        ("did_rule", "3"),
        ("screen_height", "1080"),
        ("screen_width", "1920"),
        ("heartbeat_duration", "0"),
        ("resp_content_type", "protobuf"),
        ("history_comment_count", "6"),
        ("client_enter", "1"),
        ("last_rtt", "150"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let mut set = |key: &str, value: &str| match params.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = value.to_string(),
        None => params.push((key.to_string(), value.to_string())),
    };
    set("compress", "gzip");
    set("room_id", room_id);
    set("internal_ext", &batch.internal_ext);
    set("cursor", &batch.cursor);
    let mut route: Vec<_> = batch
        .route_params
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .collect();
    route.sort();
    for (k, v) in route {
        set(k, v);
    }
    let query: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect();
    format!(
        "{}?{}&version_code=270000",
        batch.push_server,
        query.join("&")
    )
}

/// Cookies posés par le serveur de signature (`name=value; ...`), sans leurs attributs.
fn cookie_header(set_cookies: &[String]) -> String {
    set_cookies
        .iter()
        .flat_map(|h| h.split(',').map(str::trim).collect::<Vec<_>>())
        .filter_map(|c| c.split(';').next())
        .filter(|c| c.contains('='))
        .collect::<Vec<_>>()
        .join("; ")
}

impl Inner {
    async fn key(&self) -> Result<String> {
        self.api_key
            .lock()
            .await
            .clone()
            .ok_or(Error::NotAuthenticated("clé d'API Euler Stream (TikTok)"))
    }

    /// `room_id` du direct en cours d'un pseudo ; `None` s'il n'est pas en direct.
    async fn room_id(&self, unique_id: &str) -> Result<Option<String>> {
        let key = self.key().await?;
        let resp = self
            .http
            .get(format!(
                "{SIGN_URL}/webcast/anchors/{}/room_id",
                encode(unique_id)
            ))
            .header("x-api-key", key)
            .header("Accept", "application/json")
            .send()
            .await?;
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::PAYMENT_REQUIRED => {
                Err(Error::Other(format!(
                    "Euler Stream refuse la clé d'API ({status}): {}",
                    body["message"].as_str().unwrap_or("accès refusé")
                )))
            }
            StatusCode::TOO_MANY_REQUESTS => {
                Err(Error::Other("Euler Stream: trop de requêtes".into()))
            }
            _ => Ok(room_id_from(&body)),
        }
    }

    /// URL signée : premier lot de messages, informations de connexion et cookies.
    async fn signed_connect(
        &self,
        room_id: &str,
    ) -> Result<(proto::FetchResult, Vec<String>, String)> {
        let key = self.key().await?;
        let resp = self
            .http
            .get(format!("{SIGN_URL}/webcast/rooms/{room_id}/connect"))
            .query(&[
                ("client", CLIENT_NAME),
                ("user_agent", USER_AGENT),
                ("client_enter", "true"),
                ("platform", "web"),
            ])
            .header("x-api-key", key)
            .header("Accept", "application/protobuf,application/json")
            .send()
            .await?;
        let status = resp.status();
        let cookies: Vec<String> = resp
            .headers()
            .get_all("x-set-tt-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok().map(str::to_owned))
            .collect();
        let real_room = resp
            .headers()
            .get("x-room-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .unwrap_or_else(|| room_id.to_string());
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            let text = String::from_utf8_lossy(&bytes);
            return Err(Error::Other(format!(
                "Euler Stream a refusé la connexion TikTok ({status}): {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        if cookies.is_empty() {
            return Err(Error::Other(
                "Euler Stream n'a pas fourni de cookies de connexion".into(),
            ));
        }
        let batch = proto::FetchResult::decode(bytes.as_ref())
            .map_err(|e| Error::Other(format!("réponse de signature illisible: {e}")))?;
        if batch.push_server.is_empty() {
            return Err(Error::Other(
                "réponse de signature sans serveur de connexion".into(),
            ));
        }
        Ok((batch, cookies, real_room))
    }
}

fn room_id_from(body: &Value) -> Option<String> {
    match &body["room_id"] {
        Value::String(s) if !s.is_empty() && s != "0" => Some(s.clone()),
        Value::Number(n) if n.as_i64().is_some_and(|n| n != 0) => Some(n.to_string()),
        _ => None,
    }
}

#[async_trait]
impl ChatConnector for TiktokConnector {
    fn platform(&self) -> Platform {
        Platform::Tiktok
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

    async fn send_message(&self, _channel: &Channel, _text: &str) -> Result<()> {
        Err(Error::Unsupported {
            platform: Platform::Tiktok,
            feature: "l'envoi de messages (lecture seule)",
        })
    }

    async fn moderate(&self, action: &ModerationAction) -> Result<()> {
        Err(Error::UnsupportedAction {
            platform: Platform::Tiktok,
            action: action.kind.name(),
        })
    }
}

async fn run_channel(inner: Arc<Inner>, channel: Channel, tx: mpsc::Sender<ChatEvent>) {
    let mut backoff = 2;
    loop {
        let started = Instant::now();
        let mut connected = false;
        let outcome = session(&inner, &channel, &tx, &mut connected).await;
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
            Ok(Ended::Offline) => OFFLINE_WAIT,
            Err(e) => {
                eprintln!("chat TikTok {}: {e}", channel.display_name);
                if started.elapsed() > Duration::from_secs(60) {
                    backoff = 2;
                }
                let wait = Duration::from_secs(backoff);
                backoff = (backoff * 2).min(120);
                wait
            }
        };
        tokio::time::sleep(wait).await;
    }
}

enum Ended {
    Offline,
}

async fn session(
    inner: &Inner,
    channel: &Channel,
    tx: &mpsc::Sender<ChatEvent>,
    connected: &mut bool,
) -> Result<Ended> {
    let net = |e: tokio_tungstenite::tungstenite::Error| Error::Network(e.to_string());
    let Some(room) = inner.room_id(&channel.external_id).await? else {
        return Ok(Ended::Offline);
    };
    let (batch, cookies, room) = inner.signed_connect(&room).await?;
    let room_num: i64 = room
        .parse()
        .map_err(|_| Error::Other(format!("room_id TikTok invalide: {room}")))?;

    let mut request = websocket_url(&batch, &room)
        .into_client_request()
        .map_err(net)?;
    let headers = request.headers_mut();
    for (name, value) in [
        ("User-Agent", USER_AGENT.to_string()),
        ("Cookie", cookie_header(&cookies)),
    ] {
        headers.insert(
            name,
            HeaderValue::from_str(&value)
                .map_err(|_| Error::Other(format!("en-tête {name} invalide")))?,
        );
    }
    let (mut ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(net)?;

    let mut unique = [0u8; 8];
    getrandom::getrandom(&mut unique)
        .map_err(|e| Error::Other(format!("aléa système indisponible: {e}")))?;
    unique[0] &= 0x7f;
    let unique = i64::from_be_bytes(unique).max(1);
    ws.send(Message::Binary(enter_room_frame(room_num, unique)))
        .await
        .map_err(net)?;
    *connected = true;
    let _ = tx
        .send(ChatEvent::Connected {
            channel_id: channel.id.clone(),
        })
        .await;

    let mut ping = tokio::time::interval(HEARTBEAT);
    ping.reset();
    let mut seq = 1i64;
    loop {
        tokio::select! {
            incoming = ws.next() => {
                let data = match incoming {
                    Some(Ok(Message::Binary(d))) => d,
                    Some(Ok(Message::Ping(p))) => {
                        ws.send(Message::Pong(p)).await.map_err(net)?;
                        continue;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        return Err(Error::Network("connexion TikTok fermée".into()))
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => return Err(net(e)),
                };
                let (batch, log_id) = match decode_frame(&data) {
                    Ok(decoded) => decoded,
                    Err(e) => {
                        eprintln!("chat TikTok {}: {e}", channel.display_name);
                        continue;
                    }
                };
                let Some(batch) = batch else { continue };
                if batch.need_ack && log_id != 0 && !batch.internal_ext.is_empty() {
                    ws.send(Message::Binary(ack_frame(log_id, &batch.internal_ext)))
                        .await
                        .map_err(net)?;
                }
                for m in &batch.messages {
                    match parse_message(&m.method, &m.payload, channel) {
                        Some(Parsed::Event(event)) => {
                            if tx.send(*event).await.is_err() {
                                return Ok(Ended::Offline);
                            }
                        }
                        Some(Parsed::StreamEnded) => return Ok(Ended::Offline),
                        None => {}
                    }
                }
            }
            _ = ping.tick() => {
                ws.send(Message::Binary(heartbeat_frame(room_num, seq))).await.map_err(net)?;
                seq += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel() -> Channel {
        Channel {
            id: "tiktok:garder".into(),
            platform: Platform::Tiktok,
            external_id: "garder".into(),
            display_name: "garder".into(),
        }
    }

    fn user(id: i64, nick: &str, handle: &str) -> proto::User {
        proto::User {
            id,
            nickname: nick.into(),
            display_id: handle.into(),
        }
    }

    fn common(id: i64, time: i64) -> proto::Common {
        proto::Common {
            msg_id: id,
            create_time: time,
            display_text: None,
        }
    }

    #[test]
    fn parses_chat_message() {
        let payload = proto::ChatMessage {
            common: Some(common(77, 1_700_000_000_000)),
            user: Some(user(5, "Fan", "fan42")),
            content: "Salut !".into(),
            user_identity: Some(proto::UserIdentity {
                is_subscriber_of_anchor: true,
                is_moderator_of_anchor: false,
                is_anchor: false,
            }),
        }
        .encode_to_vec();
        let Some(Parsed::Event(boxed)) = parse_message("WebcastChatMessage", &payload, &channel())
        else {
            panic!("attendu un message")
        };
        let ChatEvent::Message(m) = *boxed else {
            panic!("mauvais type d'événement")
        };
        assert_eq!((m.id.as_str(), m.text.as_str()), ("77", "Salut !"));
        assert_eq!(m.author.external_id, "fan42");
        assert_eq!(m.author.display_name, "Fan");
        assert!(m.author.is_subscriber && !m.author.is_moderator);
        assert_eq!(m.timestamp_ms, 1_700_000_000_000);
    }

    #[test]
    fn ignores_empty_chat_and_unknown_methods() {
        let payload = proto::ChatMessage::default().encode_to_vec();
        assert!(parse_message("WebcastChatMessage", &payload, &channel()).is_none());
        assert!(parse_message("WebcastRoomUserSeqMessage", &[1, 2, 3], &channel()).is_none());
    }

    #[test]
    fn counts_only_the_last_message_of_a_gift_streak() {
        let gift = |kind, repeat_end| {
            proto::GiftMessage {
                common: Some(common(1, 0)),
                repeat_count: 3,
                user: Some(user(5, "Fan", "fan42")),
                repeat_end,
                gift: Some(proto::Gift {
                    kind,
                    diamond_count: 1,
                    name: "Rose".into(),
                }),
            }
            .encode_to_vec()
        };
        assert!(
            parse_message("WebcastGiftMessage", &gift(GIFT_TYPE_STREAK, 0), &channel()).is_none()
        );
        let Some(Parsed::Event(boxed)) =
            parse_message("WebcastGiftMessage", &gift(GIFT_TYPE_STREAK, 1), &channel())
        else {
            panic!("attendu un cadeau")
        };
        let ChatEvent::Event(e) = *boxed else {
            panic!("mauvais type d'événement")
        };
        assert_eq!(e.message.as_deref(), Some("Rose x3"));
        assert_eq!(e.who, "Fan");
        // Un cadeau hors série compte immédiatement.
        assert!(parse_message("WebcastGiftMessage", &gift(2, 0), &channel()).is_some());
    }

    #[test]
    fn detects_follows_and_stream_end() {
        let social = |key: &str| {
            proto::SocialMessage {
                common: Some(proto::Common {
                    display_text: Some(proto::Text { key: key.into() }),
                    ..common(2, 0)
                }),
                user: Some(user(5, "Fan", "fan42")),
            }
            .encode_to_vec()
        };
        let Some(Parsed::Event(boxed)) = parse_message(
            "WebcastSocialMessage",
            &social("pm_main_follow_message_viewer_2"),
            &channel(),
        ) else {
            panic!("attendu un follow")
        };
        let ChatEvent::Event(e) = *boxed else {
            panic!("mauvais type d'événement")
        };
        assert_eq!(e.kind, StreamEventKind::Follow);
        assert!(parse_message(
            "WebcastSocialMessage",
            &social("pm_mt_guidance_share"),
            &channel()
        )
        .is_none());

        let end = proto::ControlMessage { action: 3 }.encode_to_vec();
        assert_eq!(
            parse_message("WebcastControlMessage", &end, &channel()),
            Some(Parsed::StreamEnded)
        );
        let other = proto::ControlMessage { action: 1 }.encode_to_vec();
        assert!(parse_message("WebcastControlMessage", &other, &channel()).is_none());
    }

    #[test]
    fn decodes_gzipped_frames_and_builds_acks() {
        use flate2::write::GzEncoder;
        use std::io::Write;
        let batch = proto::FetchResult {
            messages: vec![proto::BaseMessage {
                method: "WebcastChatMessage".into(),
                payload: vec![1],
            }],
            internal_ext: "ext".into(),
            need_ack: true,
            ..Default::default()
        };
        let mut gz = GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&batch.encode_to_vec()).unwrap();
        let frame = proto::PushFrame {
            log_id: 99,
            payload_encoding: "pb".into(),
            payload_type: "msg".into(),
            payload: gz.finish().unwrap(),
            ..Default::default()
        }
        .encode_to_vec();
        let (decoded, log_id) = decode_frame(&frame).unwrap();
        assert_eq!(log_id, 99);
        assert_eq!(decoded.unwrap(), batch);

        let ack = proto::PushFrame::decode(ack_frame(99, "ext").as_slice()).unwrap();
        assert_eq!((ack.log_id, ack.payload_type.as_str()), (99, "ack"));
        assert_eq!(ack.payload, b"ext");
        // Une trame sans lot (ex. heartbeat) n'a rien à décoder.
        let hb = proto::PushFrame::decode(heartbeat_frame(5, 2).as_slice()).unwrap();
        assert_eq!(hb.payload_type, "hb");
        assert!(decode_frame(&heartbeat_frame(5, 2)).unwrap().0.is_none());
    }

    #[test]
    fn enter_room_frame_targets_the_room() {
        let frame = proto::PushFrame::decode(enter_room_frame(123, 456).as_slice()).unwrap();
        assert_eq!(frame.payload_type, "im_enter_room");
        let msg = proto::EnterRoom::decode(frame.payload.as_slice()).unwrap();
        assert_eq!(
            (msg.room_id, msg.enter_unique_id, msg.live_id),
            (123, 456, 12)
        );
        assert_eq!(msg.identity, "audience");
    }

    #[test]
    fn builds_websocket_url_with_route_params() {
        let batch = proto::FetchResult {
            push_server: "wss://push.example/ws/".into(),
            cursor: "c1".into(),
            internal_ext: "a b".into(),
            route_params: HashMap::from([
                ("im_path".to_string(), "/x".to_string()),
                ("empty".to_string(), String::new()),
            ]),
            ..Default::default()
        };
        let url = websocket_url(&batch, "42");
        assert!(url.starts_with("wss://push.example/ws/?version_code=180800&aid=1988"));
        assert!(url.contains("&room_id=42"));
        assert!(url.contains("&internal_ext=a%20b"));
        assert!(url.contains("&im_path=%2Fx"));
        assert!(!url.contains("empty="));
        assert!(url.ends_with("&version_code=270000"));
    }

    #[test]
    fn extracts_cookies_and_room_ids() {
        let headers = vec![
            "ttwid=abc; Path=/; Secure".to_string(),
            "msToken=xyz; Path=/".to_string(),
        ];
        assert_eq!(cookie_header(&headers), "ttwid=abc; msToken=xyz");
        assert_eq!(
            room_id_from(&serde_json::json!({"room_id":"123"})).as_deref(),
            Some("123")
        );
        assert_eq!(
            room_id_from(&serde_json::json!({"room_id":456})).as_deref(),
            Some("456")
        );
        assert!(room_id_from(&serde_json::json!({"room_id":"0"})).is_none());
        assert!(room_id_from(&serde_json::json!({"ok":false})).is_none());
    }
}
