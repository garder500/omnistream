//! Analyse des réponses `liveChatMessages.list` de l'API YouTube. Fonctions pures, sans I/O.

use serde_json::Value;

use crate::connectors::ChatEvent;
use crate::domain::{Channel, ChatAuthor, ChatMessage, Platform, StreamEvent, StreamEventKind};

/// Une page de messages et la suite du sondage.
#[derive(Debug, PartialEq)]
pub struct Page {
    pub events: Vec<ChatEvent>,
    pub next_page_token: Option<String>,
    /// Délai demandé par YouTube avant le prochain appel, en millisecondes.
    pub poll_ms: u64,
}

/// Millisecondes epoch Unix d'une date RFC 3339 (`2026-01-02T03:04:05.678Z` ou avec décalage).
pub fn parse_rfc3339_ms(s: &str) -> Option<i64> {
    let (date, rest) = s.split_once('T')?;
    let mut d = date.split('-');
    let (y, m, day): (i64, i64, i64) = (
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
    );
    let split = rest.find(['Z', '+', '-'])?;
    let (clock, zone) = rest.split_at(split);
    let mut c = clock.split(':');
    let (h, min): (i64, i64) = (c.next()?.parse().ok()?, c.next()?.parse().ok()?);
    let sec_text = c.next()?;
    let (sec, frac) = sec_text.split_once('.').unwrap_or((sec_text, ""));
    let sec: i64 = sec.parse().ok()?;
    let millis: i64 = format!("{frac:0<3}")[..3].parse().ok()?;
    let offset = if zone == "Z" {
        0
    } else {
        let sign = if zone.starts_with('-') { -1 } else { 1 };
        let (oh, om) = zone[1..].split_once(':')?;
        sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60)
    };
    // Jours depuis 1970-01-01 (algorithme « civil from days » de H. Hinnant, à l'envers).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + h * 3600 + min * 60 + sec - offset) * 1000) + millis)
}

fn parse_item(item: &Value, channel: &Channel) -> Option<ChatEvent> {
    let snippet = &item["snippet"];
    let id = item["id"].as_str()?.to_string();
    let kind = snippet["type"].as_str()?;
    if kind == "messageDeletedEvent" {
        return Some(ChatEvent::MessageDeleted {
            channel_id: channel.id.clone(),
            message_id: snippet["messageDeletedDetails"]["deletedMessageId"]
                .as_str()?
                .to_string(),
        });
    }
    let timestamp_ms = snippet["publishedAt"]
        .as_str()
        .and_then(parse_rfc3339_ms)
        .unwrap_or(0);
    let author = &item["authorDetails"];
    let who = author["displayName"].as_str().unwrap_or("anonyme");
    let comment = |details: &str| {
        snippet[details]["userComment"]
            .as_str()
            .filter(|c| !c.is_empty())
            .map(str::to_owned)
    };
    let event = |kind: StreamEventKind, message: Option<String>| {
        Some(ChatEvent::Event(StreamEvent {
            id: id.clone(),
            channel_id: channel.id.clone(),
            platform: Platform::Youtube,
            who: who.to_string(),
            kind,
            message,
            timestamp_ms,
        }))
    };
    match kind {
        "textMessageEvent" => {
            let text = snippet["textMessageDetails"]["messageText"]
                .as_str()
                .or(snippet["displayMessage"].as_str())
                .filter(|t| !t.is_empty())?;
            let flag = |k: &str| author[k].as_bool().unwrap_or(false);
            Some(ChatEvent::Message(ChatMessage {
                id,
                channel_id: channel.id.clone(),
                platform: Platform::Youtube,
                author: ChatAuthor {
                    external_id: author["channelId"].as_str().unwrap_or_default().to_string(),
                    display_name: who.to_string(),
                    is_moderator: flag("isChatModerator") || flag("isChatOwner"),
                    is_subscriber: flag("isChatSponsor"),
                },
                text: text.to_string(),
                timestamp_ms,
                emotes: vec![],
                badges: vec![],
                room_id: None,
            }))
        }
        "superChatEvent" | "superStickerEvent" => {
            let details = if kind == "superChatEvent" {
                "superChatDetails"
            } else {
                "superStickerDetails"
            };
            let micros: u64 = snippet[details]["amountMicros"].as_str()?.parse().ok()?;
            event(
                StreamEventKind::Donation {
                    amount_cents: micros / 10_000,
                    currency: snippet[details]["currency"]
                        .as_str()
                        .unwrap_or("USD")
                        .to_string(),
                },
                comment(details),
            )
        }
        "newSponsorEvent" => event(StreamEventKind::Sub { months: 1 }, None),
        "memberMilestoneChatEvent" => event(
            StreamEventKind::Sub {
                months: snippet["memberMilestoneChatDetails"]["memberMonth"]
                    .as_u64()
                    .unwrap_or(1) as u32,
            },
            comment("memberMilestoneChatDetails"),
        ),
        "membershipGiftingEvent" => event(StreamEventKind::Gift { recipient: None }, None),
        _ => None,
    }
}

pub fn parse_page(v: &Value, channel: &Channel) -> Page {
    Page {
        events: v["items"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| parse_item(i, channel))
                    .collect()
            })
            .unwrap_or_default(),
        next_page_token: v["nextPageToken"].as_str().map(str::to_owned),
        poll_ms: v["pollingIntervalMillis"]
            .as_u64()
            .unwrap_or(5000)
            .clamp(2000, 60_000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn channel() -> Channel {
        Channel {
            id: "youtube:garder500".into(),
            platform: Platform::Youtube,
            external_id: "garder500".into(),
            display_name: "garder500".into(),
        }
    }

    #[test]
    fn parses_dates() {
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_rfc3339_ms("2023-11-14T22:13:20.123456+00:00"),
            Some(1_700_000_000_123)
        );
        assert_eq!(
            parse_rfc3339_ms("2023-11-15T00:13:20+02:00"),
            Some(1_700_000_000_000)
        );
        assert_eq!(parse_rfc3339_ms("n'importe quoi"), None);
    }

    #[test]
    fn parses_messages_and_events() {
        let raw = json!({
            "nextPageToken": "tok2",
            "pollingIntervalMillis": 4500,
            "items": [
                {"id":"m1","snippet":{"type":"textMessageEvent","publishedAt":"2023-11-14T22:13:20Z",
                    "textMessageDetails":{"messageText":"Salut !"}},
                 "authorDetails":{"channelId":"UCfan","displayName":"Fan","isChatModerator":true,"isChatSponsor":false}},
                {"id":"m2","snippet":{"type":"superChatEvent","publishedAt":"2023-11-14T22:13:21Z",
                    "superChatDetails":{"amountMicros":"5000000","currency":"EUR","userComment":"bravo"}},
                 "authorDetails":{"channelId":"UCgen","displayName":"Généreux"}},
                {"id":"m3","snippet":{"type":"memberMilestoneChatEvent","publishedAt":"2023-11-14T22:13:22Z",
                    "memberMilestoneChatDetails":{"memberMonth":6}},
                 "authorDetails":{"channelId":"UCmem","displayName":"Membre"}},
                {"id":"m4","snippet":{"type":"messageDeletedEvent","messageDeletedDetails":{"deletedMessageId":"m1"}}},
                {"id":"m5","snippet":{"type":"chatEndedEvent"}}
            ]
        });
        let page = parse_page(&raw, &channel());
        assert_eq!(page.next_page_token.as_deref(), Some("tok2"));
        assert_eq!(page.poll_ms, 4500);
        assert_eq!(page.events.len(), 4);
        let ChatEvent::Message(m) = &page.events[0] else {
            panic!()
        };
        assert_eq!((m.id.as_str(), m.text.as_str()), ("m1", "Salut !"));
        assert_eq!(m.author.external_id, "UCfan");
        assert!(m.author.is_moderator && !m.author.is_subscriber);
        assert_eq!(m.timestamp_ms, 1_700_000_000_000);
        let ChatEvent::Event(e) = &page.events[1] else {
            panic!()
        };
        assert_eq!(
            e.kind,
            StreamEventKind::Donation {
                amount_cents: 500,
                currency: "EUR".into()
            }
        );
        assert_eq!(e.message.as_deref(), Some("bravo"));
        let ChatEvent::Event(e) = &page.events[2] else {
            panic!()
        };
        assert_eq!(e.kind, StreamEventKind::Sub { months: 6 });
        assert_eq!(
            page.events[3],
            ChatEvent::MessageDeleted {
                channel_id: "youtube:garder500".into(),
                message_id: "m1".into()
            }
        );
    }

    #[test]
    fn clamps_polling_interval() {
        assert_eq!(
            parse_page(&json!({"pollingIntervalMillis": 10}), &channel()).poll_ms,
            2000
        );
        assert_eq!(parse_page(&json!({}), &channel()).poll_ms, 5000);
    }
}
