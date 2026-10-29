//! Analyse des trames JSON du chat websocket Trovo. Fonctions pures, sans I/O.

use serde_json::Value;

use crate::connectors::ChatEvent;
use crate::domain::{Channel, ChatAuthor, ChatMessage, Platform, StreamEvent, StreamEventKind};

/// Trame reçue du serveur de chat.
#[derive(Debug, PartialEq)]
pub enum Frame {
    /// Réponse à une commande (AUTH...). `error` est `None` en cas de succès.
    Response { nonce: String, error: Option<String> },
    /// Réponse au PING, avec l'intervalle (secondes) à respecter entre deux PING.
    Pong { gap: u64 },
    /// Messages de chat et événements, avec l'identifiant de l'expéditeur
    /// de chaque message (nécessaire pour la suppression).
    Chat(Vec<ParsedChat>),
    Other,
}

#[derive(Debug, PartialEq)]
pub struct ParsedChat {
    pub event: ChatEvent,
    /// `(message_id, sender_id)` pour les messages normaux.
    pub sender: Option<(String, String)>,
}

fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn parse_frame(raw: &str, channel: &Channel) -> Frame {
    let Ok(v) = serde_json::from_str::<Value>(raw) else { return Frame::Other };
    match v["type"].as_str() {
        Some("RESPONSE") => Frame::Response {
            nonce: v["nonce"].as_str().unwrap_or_default().to_string(),
            error: v["error"].as_str().filter(|e| !e.is_empty()).map(str::to_owned),
        },
        Some("PONG") => Frame::Pong { gap: v["data"]["gap"].as_u64().unwrap_or(30).clamp(5, 120) },
        Some("CHAT") => Frame::Chat(
            v["data"]["chats"]
                .as_array()
                .map(|chats| chats.iter().filter_map(|c| parse_chat(c, channel)).collect())
                .unwrap_or_default(),
        ),
        _ => Frame::Other,
    }
}

fn parse_chat(c: &Value, channel: &Channel) -> Option<ParsedChat> {
    let kind = c["type"].as_i64().unwrap_or(0);
    let login = text(&c["user_name"]);
    let nick = text(&c["nick_name"]);
    let who = nick.clone().or_else(|| login.clone()).unwrap_or_else(|| "anonyme".into());
    let content = c["content"].as_str().unwrap_or_default().to_string();
    let id = text(&c["message_id"]).unwrap_or_else(|| format!("{}-{kind}-{who}-{}", channel.id, text(&c["send_time"]).unwrap_or_default()));
    let timestamp_ms = c["send_time"].as_i64().map(|s| s * 1000).unwrap_or(0);

    let event_kind = match kind {
        // Abonnement, puis cadeaux d'abonnements (aléatoires ou ciblés).
        5 => Some(StreamEventKind::Sub { months: 1 }),
        9 | 10 => Some(StreamEventKind::Gift { recipient: None }),
        _ => None,
    };
    if let Some(kind) = event_kind {
        return Some(ParsedChat {
            event: ChatEvent::Event(StreamEvent {
                id,
                channel_id: channel.id.clone(),
                platform: Platform::Trovo,
                who,
                kind,
                message: Some(content).filter(|m| !m.is_empty()),
                timestamp_ms,
            }),
            sender: None,
        });
    }
    // 0 : message, 1 à 4 : messages « magic chat » ; le reste (système, bienvenue, follow) est ignoré.
    if !(0..=4).contains(&kind) || content.is_empty() {
        return None;
    }
    let roles: Vec<&str> = c["roles"].as_array().map(|r| r.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let sender_id = text(&c["sender_id"]).or_else(|| text(&c["uid"]));
    let external_id = login.or(nick).unwrap_or_default();
    Some(ParsedChat {
        sender: sender_id.map(|s| (id.clone(), s)),
        event: ChatEvent::Message(ChatMessage {
            id,
            channel_id: channel.id.clone(),
            platform: Platform::Trovo,
            author: ChatAuthor {
                external_id,
                display_name: who,
                is_moderator: roles.iter().any(|r| matches!(*r, "mod" | "supermod" | "streamer" | "admin" | "editor")),
                is_subscriber: text(&c["sub_lv"]).is_some() || roles.contains(&"subscriber"),
            },
            text: content,
            timestamp_ms,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel() -> Channel {
        Channel { id: "trovo:garder".into(), platform: Platform::Trovo, external_id: "garder".into(), display_name: "garder".into() }
    }

    #[test]
    fn parses_auth_response_and_pong() {
        assert_eq!(
            parse_frame(r#"{"type":"RESPONSE","nonce":"auth","error":""}"#, &channel()),
            Frame::Response { nonce: "auth".into(), error: None }
        );
        assert_eq!(
            parse_frame(r#"{"type":"RESPONSE","nonce":"auth","error":"invalid token"}"#, &channel()),
            Frame::Response { nonce: "auth".into(), error: Some("invalid token".into()) }
        );
        assert_eq!(parse_frame(r#"{"type":"PONG","nonce":"x","data":{"gap":25}}"#, &channel()), Frame::Pong { gap: 25 });
        assert_eq!(parse_frame("pas du json", &channel()), Frame::Other);
    }

    #[test]
    fn parses_chat_messages() {
        let raw = r#"{"type":"CHAT","data":{"channel_id":"456","chats":[
            {"type":0,"content":"Salut !","nick_name":"Fan","user_name":"fan42","message_id":"m1","sender_id":99,"send_time":1700000000,"roles":["mod"],"sub_lv":"2"},
            {"type":0,"content":"yo","nick_name":"Lurker","message_id":"m2","sender_id":100,"send_time":1700000001,"roles":[]},
            {"type":8,"content":"bienvenue","nick_name":"Fan","message_id":"m3"},
            {"type":5,"content":"vient de s'abonner","nick_name":"Fan","message_id":"m4","send_time":1700000002}
        ]}}"#;
        let Frame::Chat(chats) = parse_frame(raw, &channel()) else { panic!("attendu CHAT") };
        assert_eq!(chats.len(), 3);
        let ChatEvent::Message(m) = &chats[0].event else { panic!() };
        assert_eq!(m.id, "m1");
        assert_eq!(m.text, "Salut !");
        assert_eq!(m.timestamp_ms, 1_700_000_000_000);
        assert_eq!(m.author.display_name, "Fan");
        assert_eq!(m.author.external_id, "fan42");
        assert!(m.author.is_moderator && m.author.is_subscriber);
        assert_eq!(chats[0].sender, Some(("m1".into(), "99".into())));
        let ChatEvent::Message(m) = &chats[1].event else { panic!() };
        assert!(!m.author.is_moderator && !m.author.is_subscriber);
        let ChatEvent::Event(e) = &chats[2].event else { panic!() };
        assert_eq!(e.kind, StreamEventKind::Sub { months: 1 });
        assert_eq!(e.who, "Fan");
    }
}
