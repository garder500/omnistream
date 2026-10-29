//! Analyse des lignes IRC (avec tags IRCv3) du chat Twitch. Fonctions pures, sans I/O.

use std::collections::HashMap;

use crate::connectors::ChatEvent;
use crate::domain::{Channel, ChatAuthor, ChatMessage, Platform, StreamEvent, StreamEventKind};
use crate::emotes::{parse_badges, parse_twitch_emotes};

#[derive(Debug, PartialEq, Eq)]
pub struct IrcLine<'a> {
    pub tags: HashMap<String, String>,
    pub prefix: Option<&'a str>,
    pub command: &'a str,
    pub params: Vec<&'a str>,
    /// Dernier paramètre introduit par `:`.
    pub trailing: Option<&'a str>,
}

fn unescape_tag(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(':') => out.push(';'),
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

pub fn parse_line(line: &str) -> Option<IrcLine<'_>> {
    let mut rest = line.trim_end_matches(['\r', '\n']);
    let mut tags = HashMap::new();
    if let Some(stripped) = rest.strip_prefix('@') {
        let (raw, tail) = stripped.split_once(' ')?;
        for pair in raw.split(';') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            tags.insert(k.to_string(), unescape_tag(v));
        }
        rest = tail.trim_start();
    }
    let mut prefix = None;
    if let Some(stripped) = rest.strip_prefix(':') {
        let (p, tail) = stripped.split_once(' ')?;
        prefix = Some(p);
        rest = tail.trim_start();
    }
    let (head, trailing) = match rest.split_once(" :") {
        Some((h, t)) => (h, Some(t)),
        None => (rest, None),
    };
    let mut parts = head.split_whitespace();
    let command = parts.next()?;
    Some(IrcLine {
        tags,
        prefix,
        command,
        params: parts.collect(),
        trailing,
    })
}

/// Un message `/me` arrive encadré par `\u{1}ACTION ... \u{1}`.
fn clean_text(text: &str) -> String {
    text.strip_prefix("\u{1}ACTION ")
        .and_then(|t| t.strip_suffix('\u{1}'))
        .unwrap_or(text)
        .to_string()
}

fn has_badge(tags: &HashMap<String, String>, names: &[&str]) -> bool {
    tags.get("badges")
        .map(|b| {
            b.split(',')
                .any(|badge| names.iter().any(|n| badge.split('/').next() == Some(n)))
        })
        .unwrap_or(false)
}

/// Traduit une ligne IRC en événement de chat, si elle en porte un.
pub fn to_event(line: &IrcLine<'_>, channel: &Channel) -> Option<ChatEvent> {
    match line.command {
        "PRIVMSG" => {
            let login = line.prefix?.split('!').next()?;
            let tags = &line.tags;
            let timestamp_ms = tags
                .get("tmi-sent-ts")
                .and_then(|t| t.parse().ok())
                .unwrap_or(0);
            let raw = line.trailing.unwrap_or("");
            let text = clean_text(raw);
            // Les indices Twitch comptent le préfixe `\u{1}ACTION ` (8 caractères) des `/me`.
            let offset = if text.len() != raw.len() { 8 } else { 0 };
            let emotes = tags
                .get("emotes")
                .map(|e| parse_twitch_emotes(e, &text, offset))
                .unwrap_or_default();
            Some(ChatEvent::Message(ChatMessage {
                id: tags.get("id")?.clone(),
                channel_id: channel.id.clone(),
                platform: Platform::Twitch,
                author: ChatAuthor {
                    external_id: tags.get("user-id").cloned().unwrap_or_default(),
                    display_name: tags
                        .get("display-name")
                        .filter(|n| !n.is_empty())
                        .cloned()
                        .unwrap_or_else(|| login.to_string()),
                    is_moderator: tags.get("mod").map(|m| m == "1").unwrap_or(false)
                        || has_badge(tags, &["moderator", "broadcaster"]),
                    is_subscriber: tags.get("subscriber").map(|m| m == "1").unwrap_or(false),
                },
                text,
                timestamp_ms,
                emotes,
                badges: tags
                    .get("badges")
                    .map(|b| parse_badges(b))
                    .unwrap_or_default(),
                room_id: tags.get("room-id").cloned().filter(|r| !r.is_empty()),
            }))
        }
        "CLEARMSG" => Some(ChatEvent::MessageDeleted {
            channel_id: channel.id.clone(),
            message_id: line.tags.get("target-msg-id")?.clone(),
        }),
        "USERNOTICE" => {
            let tags = &line.tags;
            let who = tags
                .get("display-name")
                .filter(|n| !n.is_empty())
                .or_else(|| tags.get("login"))?
                .clone();
            let months = tags
                .get("msg-param-cumulative-months")
                .and_then(|m| m.parse().ok());
            let kind = match tags.get("msg-id")?.as_str() {
                "sub" => StreamEventKind::Sub {
                    months: months.unwrap_or(1),
                },
                "resub" => StreamEventKind::Sub {
                    months: months.unwrap_or(1),
                },
                "subgift" | "anonsubgift" => StreamEventKind::Gift {
                    recipient: tags.get("msg-param-recipient-display-name").cloned(),
                },
                "raid" => StreamEventKind::Raid {
                    viewers: tags
                        .get("msg-param-viewerCount")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                },
                _ => return None,
            };
            Some(ChatEvent::Event(StreamEvent {
                id: tags.get("id").cloned().unwrap_or_default(),
                channel_id: channel.id.clone(),
                platform: Platform::Twitch,
                who,
                kind,
                message: line.trailing.map(str::to_string).filter(|t| !t.is_empty()),
                timestamp_ms: tags
                    .get("tmi-sent-ts")
                    .and_then(|t| t.parse().ok())
                    .unwrap_or(0),
            }))
        }
        _ => None,
    }
}

/// Un PRIVMSG portant le tag `bits` est aussi un cheer : on en tire un événement
/// en plus du message de chat (le message lui-même reste inchangé).
pub fn bits_event(line: &IrcLine<'_>, channel: &Channel) -> Option<ChatEvent> {
    if line.command != "PRIVMSG" {
        return None;
    }
    let tags = &line.tags;
    let amount: u32 = tags.get("bits")?.parse().ok().filter(|b| *b > 0)?;
    let login = line.prefix?.split('!').next()?;
    let who = tags
        .get("display-name")
        .filter(|n| !n.is_empty())
        .cloned()
        .unwrap_or_else(|| login.to_string());
    let timestamp_ms: i64 = tags
        .get("tmi-sent-ts")
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    Some(ChatEvent::Event(StreamEvent {
        id: tags
            .get("id")
            .map(|i| format!("bits:{i}"))
            .unwrap_or_else(|| format!("bits:{login}:{timestamp_ms}")),
        channel_id: channel.id.clone(),
        platform: Platform::Twitch,
        who,
        kind: StreamEventKind::Bits { amount },
        message: Some(clean_text(line.trailing.unwrap_or(""))).filter(|t| !t.is_empty()),
        timestamp_ms,
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn cheer_yields_a_bits_event() {
        let raw = "@bits=100;display-name=Fan;id=m7;tmi-sent-ts=9 :fan!fan@fan.tmi.twitch.tv PRIVMSG #garder :Cheer100 gg";
        let line = parse_line(raw).unwrap();
        let Some(ChatEvent::Event(e)) = bits_event(&line, &channel()) else {
            panic!("événement attendu")
        };
        assert_eq!(e.kind, StreamEventKind::Bits { amount: 100 });
        assert_eq!(e.who, "Fan");
        assert_eq!(e.id, "bits:m7");
        let plain =
            parse_line("@id=m8;tmi-sent-ts=9 :fan!fan@fan.tmi.twitch.tv PRIVMSG #garder :salut")
                .unwrap();
        assert!(bits_event(&plain, &channel()).is_none());
    }

    use super::*;

    fn channel() -> Channel {
        Channel {
            id: "twitch:garder".into(),
            platform: Platform::Twitch,
            external_id: "garder".into(),
            display_name: "garder".into(),
        }
    }

    #[test]
    fn parses_privmsg_with_tags() {
        let raw = "@badge-info=;badges=moderator/1;display-name=Luna\\sPlays;id=abc-1;mod=1;subscriber=1;tmi-sent-ts=1700000000000;user-id=42 :lunaplays!lunaplays@lunaplays.tmi.twitch.tv PRIVMSG #garder :GG pour le boss : bien joué\r\n";
        let line = parse_line(raw).unwrap();
        assert_eq!(line.command, "PRIVMSG");
        assert_eq!(line.params, vec!["#garder"]);
        let Some(ChatEvent::Message(m)) = to_event(&line, &channel()) else {
            panic!("message attendu")
        };
        assert_eq!(m.id, "abc-1");
        assert_eq!(m.author.display_name, "Luna Plays");
        assert_eq!(m.author.external_id, "42");
        assert!(m.author.is_moderator && m.author.is_subscriber);
        assert_eq!(m.text, "GG pour le boss : bien joué");
        assert_eq!(m.timestamp_ms, 1_700_000_000_000);
    }

    #[test]
    fn extracts_native_emotes_and_badges() {
        let raw = "@badges=subscriber/6,vip/1;emotes=25:0-4;id=e1;room-id=99;user-id=1 :a!a@a.tmi.twitch.tv PRIVMSG #c :Kappa ok";
        let Some(ChatEvent::Message(m)) = to_event(&parse_line(raw).unwrap(), &channel()) else {
            panic!()
        };
        assert_eq!(m.emotes.len(), 1);
        assert_eq!(m.emotes[0].name, "Kappa");
        assert_eq!(m.badges.len(), 2);
        assert_eq!(m.room_id.as_deref(), Some("99"));
        let me = "@emotes=25:8-12;id=e2 :a!a@a.tmi.twitch.tv PRIVMSG #c :\u{1}ACTION Kappa\u{1}";
        let Some(ChatEvent::Message(m)) = to_event(&parse_line(me).unwrap(), &channel()) else {
            panic!()
        };
        assert_eq!(m.text, "Kappa");
        assert_eq!(m.emotes[0].name, "Kappa");
    }

    #[test]
    fn strips_action_wrapper() {
        let raw = "@id=x;user-id=1 :a!a@a.tmi.twitch.tv PRIVMSG #c :\u{1}ACTION danse\u{1}";
        let Some(ChatEvent::Message(m)) = to_event(&parse_line(raw).unwrap(), &channel()) else {
            panic!()
        };
        assert_eq!(m.text, "danse");
    }

    #[test]
    fn parses_ping_and_clearmsg() {
        let ping = parse_line("PING :tmi.twitch.tv").unwrap();
        assert_eq!(
            (ping.command, ping.trailing),
            ("PING", Some("tmi.twitch.tv"))
        );
        let clear =
            parse_line("@login=troll;target-msg-id=m9 :tmi.twitch.tv CLEARMSG #garder :bad")
                .unwrap();
        assert!(matches!(
            to_event(&clear, &channel()),
            Some(ChatEvent::MessageDeleted { message_id, .. }) if message_id == "m9"
        ));
    }

    #[test]
    fn maps_raid_notice() {
        let raw = "@display-name=Speedy;id=n1;msg-id=raid;msg-param-viewerCount=85;tmi-sent-ts=5 :tmi.twitch.tv USERNOTICE #garder";
        let Some(ChatEvent::Event(e)) = to_event(&parse_line(raw).unwrap(), &channel()) else {
            panic!()
        };
        assert_eq!(e.who, "Speedy");
        assert_eq!(e.kind, StreamEventKind::Raid { viewers: 85 });
    }
}
