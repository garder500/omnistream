//! Types de domaine partagés. Volontairement indépendants de toute plateforme :
//! ajouter un service = ajouter une variante de [`Platform`] + un connecteur.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Twitch,
    Youtube,
    Kick,
    Tiktok,
    /// Plateforme tierce (ex. RTMP générique) identifiée par un slug.
    Other(String),
}

impl Platform {
    pub fn as_str(&self) -> &str {
        match self {
            Platform::Twitch => "twitch",
            Platform::Youtube => "youtube",
            Platform::Kick => "kick",
            Platform::Tiktok => "tiktok",
            Platform::Other(slug) => slug,
        }
    }
}

impl From<&str> for Platform {
    fn from(s: &str) -> Self {
        match s {
            "twitch" => Platform::Twitch,
            "youtube" => Platform::Youtube,
            "kick" => Platform::Kick,
            "tiktok" => Platform::Tiktok,
            other => Platform::Other(other.to_string()),
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Un compte connecté (résultat d'un OAuth).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Identifiant plateforme de l'utilisateur (ex. user id Twitch).
    pub id: String,
    pub platform: Platform,
    pub display_name: String,
    /// Identifiant de connexion en minuscules (ex. login Twitch).
    pub login: String,
}

/// Une chaîne sur une plateforme, dont on lit le chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub platform: Platform,
    /// Identifiant propre à la plateforme (login Twitch, chat id YouTube...).
    pub external_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatAuthor {
    pub external_id: String,
    pub display_name: String,
    pub is_moderator: bool,
    pub is_subscriber: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub channel_id: String,
    pub platform: Platform,
    pub author: ChatAuthor,
    pub text: String,
    /// Millisecondes depuis l'epoch Unix.
    pub timestamp_ms: i64,
}

/// Événement de live (abonnement, don de subs, raid...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEventKind {
    Follow,
    Sub { months: u32 },
    Gift { recipient: Option<String> },
    Raid { viewers: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamEvent {
    pub id: String,
    pub channel_id: String,
    pub platform: Platform,
    pub who: String,
    #[serde(flatten)]
    pub kind: StreamEventKind,
    pub message: Option<String>,
    pub timestamp_ms: i64,
}

/// État du direct d'une chaîne.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamInfo {
    pub channel_id: String,
    pub title: String,
    pub viewers: u32,
    /// RFC 3339.
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModerationKind {
    DeleteMessage { message_id: String },
    Timeout { user_id: String, seconds: u32 },
    Ban { user_id: String, reason: Option<String> },
    Unban { user_id: String },
}

impl ModerationKind {
    pub fn name(&self) -> &'static str {
        match self {
            ModerationKind::DeleteMessage { .. } => "delete_message",
            ModerationKind::Timeout { .. } => "timeout",
            ModerationKind::Ban { .. } => "ban",
            ModerationKind::Unban { .. } => "unban",
        }
    }
}

/// Action de modération ciblant une chaîne donnée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationAction {
    pub channel: Channel,
    #[serde(flatten)]
    pub kind: ModerationKind,
}

/// Cible de diffusion (ex. un endpoint RTMP Twitch/YouTube/Kick).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamDestination {
    pub id: String,
    pub platform: Platform,
    pub name: String,
    pub rtmp_url: String,
    /// À terme stockée dans le trousseau de l'OS plutôt qu'en base.
    pub stream_key: String,
    pub enabled: bool,
}

impl StreamDestination {
    pub fn full_url(&self) -> String {
        format!("{}/{}", self.rtmp_url.trim_end_matches('/'), self.stream_key)
    }
}
