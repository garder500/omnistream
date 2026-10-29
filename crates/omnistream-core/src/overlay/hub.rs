//! Concentrateur de diffusion des overlays.
//!
//! Chaque overlay correspond à un *canal* nommé (`chat`, `alerts`...). Tout le
//! monde peut publier sur un canal avec [`OverlayHub::publish`] ; les pages
//! navigateur y sont abonnées via WebSocket (`/ws/<canal>`).
//!
//! Enveloppe envoyée aux pages : `{"channel": "...", "type": "...", "data": ...}`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::domain::ChatMessage;

/// Canal du chat unifié.
pub const CHAT_CHANNEL: &str = "chat";
/// Canal réservé aux alertes (abonnements, raids...). Seule la diffusion générique
/// est prévue ici : une autre unité fournira la page `/overlay/alerts`.
pub const ALERTS_CHANNEL: &str = "alerts";

const CHANNEL_CAPACITY: usize = 256;
const HISTORY_LEN: usize = 100;

/// Émote à afficher à la place d'un mot du message (si la plateforme la fournit).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emote {
    pub code: String,
    pub url: String,
}

/// Message tel que l'overlay l'affiche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayMessage {
    pub id: String,
    pub platform: String,
    pub author: OverlayAuthor,
    pub text: String,
    pub timestamp_ms: i64,
    #[serde(default)]
    pub emotes: Vec<Emote>,
    #[serde(default)]
    pub highlighted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayAuthor {
    pub display_name: String,
    #[serde(default)]
    pub is_moderator: bool,
    #[serde(default)]
    pub is_subscriber: bool,
}

impl From<&ChatMessage> for OverlayMessage {
    fn from(m: &ChatMessage) -> Self {
        Self {
            id: m.id.clone(),
            platform: m.platform.to_string(),
            author: OverlayAuthor {
                display_name: m.author.display_name.clone(),
                is_moderator: m.author.is_moderator,
                is_subscriber: m.author.is_subscriber,
            },
            text: m.text.clone(),
            timestamp_ms: m.timestamp_ms,
            emotes: Vec::new(),
            highlighted: false,
        }
    }
}

/// Réglages de l'overlay chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySettings {
    /// `orbite`, `minimal` ou `clair`.
    pub theme: String,
    /// CSS ajouté après le thème, pour la personnalisation.
    pub custom_css: String,
    pub max_messages: usize,
    pub show_platform: bool,
    pub font_size: u32,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            theme: "orbite".into(),
            custom_css: String::new(),
            max_messages: 20,
            show_platform: true,
            font_size: 22,
        }
    }
}

impl OverlaySettings {
    /// Ramène les valeurs dans des bornes raisonnables.
    pub fn sanitized(mut self) -> Self {
        if !["orbite", "minimal", "clair"].contains(&self.theme.as_str()) {
            self.theme = "orbite".into();
        }
        self.max_messages = self.max_messages.clamp(1, 100);
        self.font_size = self.font_size.clamp(10, 96);
        self
    }

    /// Feuille de style complète servie à la page : thème intégré puis CSS perso.
    pub fn css(&self) -> String {
        let base = match self.theme.as_str() {
            "minimal" => include_str!("assets/theme-minimal.css"),
            "clair" => include_str!("assets/theme-clair.css"),
            _ => include_str!("assets/theme-orbite.css"),
        };
        format!("{base}\n/* CSS personnalisé */\n{}\n", self.custom_css)
    }
}

#[derive(Default)]
struct ChatState {
    history: Vec<OverlayMessage>,
    pinned: Option<OverlayMessage>,
}

struct Inner {
    channels: HashMap<String, broadcast::Sender<String>>,
    chat: ChatState,
    settings: OverlaySettings,
}

/// Point d'entrée de diffusion, clonable à volonté.
#[derive(Clone)]
pub struct OverlayHub {
    inner: Arc<Mutex<Inner>>,
}

impl Default for OverlayHub {
    fn default() -> Self {
        Self::new()
    }
}

fn envelope(channel: &str, kind: &str, data: &Value) -> String {
    json!({ "channel": channel, "type": kind, "data": data }).to_string()
}

impl OverlayHub {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                channels: HashMap::new(),
                chat: ChatState::default(),
                settings: OverlaySettings::default(),
            })),
        }
    }

    /// API générique : diffuse `{type, data}` à toutes les pages abonnées au canal.
    /// Sans abonné, l'événement est simplement perdu. Retourne le nombre de pages jointes.
    pub fn publish<T: Serialize>(&self, channel: &str, kind: &str, data: &T) -> usize {
        let value = serde_json::to_value(data).unwrap_or(Value::Null);
        let mut inner = self.inner.lock().unwrap();
        Self::send(&mut inner, channel, envelope(channel, kind, &value))
    }

    fn send(inner: &mut Inner, channel: &str, payload: String) -> usize {
        match inner.channels.get(channel) {
            Some(tx) => tx.send(payload).unwrap_or(0),
            None => 0,
        }
    }

    /// S'abonne à un canal (créé à la demande).
    pub fn subscribe(&self, channel: &str) -> broadcast::Receiver<String> {
        let mut inner = self.inner.lock().unwrap();
        inner
            .channels
            .entry(channel.to_string())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
            .subscribe()
    }

    /// Message envoyé à la connexion d'une page (et après un décrochage), pour qu'elle
    /// retrouve son état : historique, message épinglé et réglages pour le chat.
    pub fn snapshot(&self, channel: &str) -> String {
        let inner = self.inner.lock().unwrap();
        if channel == CHAT_CHANNEL {
            let data = json!({
                "messages": inner.chat.history,
                "pinned": inner.chat.pinned,
                "settings": inner.settings,
            });
            envelope(channel, "snapshot", &data)
        } else {
            envelope(channel, "hello", &Value::Null)
        }
    }

    // --- Chat -----------------------------------------------------------

    pub fn chat_message(&self, message: OverlayMessage) {
        let mut inner = self.inner.lock().unwrap();
        // Un connecteur qui se reconnecte peut renvoyer un message déjà affiché.
        if inner.chat.history.iter().any(|m| m.id == message.id) {
            return;
        }
        inner.chat.history.push(message.clone());
        if inner.chat.history.len() > HISTORY_LEN {
            inner.chat.history.remove(0);
        }
        let payload = envelope(CHAT_CHANNEL, "message", &json!(message));
        Self::send(&mut inner, CHAT_CHANNEL, payload);
    }

    /// Raccourci pour un message du domaine.
    pub fn chat_domain_message(&self, message: &ChatMessage) {
        self.chat_message(message.into());
    }

    /// Épingle un message (remplace l'éventuel message déjà épinglé).
    pub fn pin(&self, message: OverlayMessage) {
        let mut inner = self.inner.lock().unwrap();
        inner.chat.pinned = Some(message.clone());
        let payload = envelope(CHAT_CHANNEL, "pin", &json!(message));
        Self::send(&mut inner, CHAT_CHANNEL, payload);
    }

    pub fn unpin(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.chat.pinned = None;
        let payload = envelope(CHAT_CHANNEL, "pin", &Value::Null);
        Self::send(&mut inner, CHAT_CHANNEL, payload);
    }

    /// Surligne (ou non) un message de l'historique.
    pub fn highlight(&self, id: &str, on: bool) {
        let mut inner = self.inner.lock().unwrap();
        let Inner { chat, .. } = &mut *inner;
        for m in chat.history.iter_mut().chain(chat.pinned.as_mut()).filter(|m| m.id == id) {
            m.highlighted = on;
        }
        let payload = envelope(CHAT_CHANNEL, "highlight", &json!({ "id": id, "on": on }));
        Self::send(&mut inner, CHAT_CHANNEL, payload);
    }

    /// Retire un message supprimé (modération) de l'overlay.
    pub fn delete(&self, id: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.chat.history.retain(|m| m.id != id);
        if inner.chat.pinned.as_ref().is_some_and(|m| m.id == id) {
            inner.chat.pinned = None;
        }
        let payload = envelope(CHAT_CHANNEL, "delete", &json!({ "id": id }));
        Self::send(&mut inner, CHAT_CHANNEL, payload);
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.chat.history.clear();
        let payload = envelope(CHAT_CHANNEL, "clear", &Value::Null);
        Self::send(&mut inner, CHAT_CHANNEL, payload);
    }

    // --- Réglages ---------------------------------------------------------

    pub fn settings(&self) -> OverlaySettings {
        self.inner.lock().unwrap().settings.clone()
    }

    pub fn set_settings(&self, settings: OverlaySettings) -> OverlaySettings {
        let settings = settings.sanitized();
        let mut inner = self.inner.lock().unwrap();
        inner.settings = settings.clone();
        let payload = envelope(CHAT_CHANNEL, "settings", &json!(settings));
        Self::send(&mut inner, CHAT_CHANNEL, payload);
        settings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str) -> OverlayMessage {
        OverlayMessage {
            id: id.into(),
            platform: "twitch".into(),
            author: OverlayAuthor { display_name: "Ana".into(), is_moderator: false, is_subscriber: false },
            text: "salut".into(),
            timestamp_ms: 0,
            emotes: vec![],
            highlighted: false,
        }
    }

    #[test]
    fn publish_reaches_subscribers_only_on_their_channel() {
        let hub = OverlayHub::new();
        let mut alerts = hub.subscribe(ALERTS_CHANNEL);
        let mut chat = hub.subscribe(CHAT_CHANNEL);
        assert_eq!(hub.publish(ALERTS_CHANNEL, "sub", &json!({"who": "Ana"})), 1);
        let got: Value = serde_json::from_str(&alerts.try_recv().unwrap()).unwrap();
        assert_eq!(got["channel"], "alerts");
        assert_eq!(got["type"], "sub");
        assert_eq!(got["data"]["who"], "Ana");
        assert!(chat.try_recv().is_err());
        assert_eq!(hub.publish("personne", "x", &Value::Null), 0);
    }

    #[test]
    fn snapshot_keeps_history_pin_and_highlight() {
        let hub = OverlayHub::new();
        hub.chat_message(msg("a"));
        hub.chat_message(msg("b"));
        hub.highlight("a", true);
        hub.pin(msg("b"));
        hub.delete("b");
        let snap: Value = serde_json::from_str(&hub.snapshot(CHAT_CHANNEL)).unwrap();
        assert_eq!(snap["data"]["messages"].as_array().unwrap().len(), 1);
        assert_eq!(snap["data"]["messages"][0]["highlighted"], true);
        assert!(snap["data"]["pinned"].is_null());
    }

    #[test]
    fn settings_are_sanitized() {
        let hub = OverlayHub::new();
        let s = hub.set_settings(OverlaySettings { theme: "inconnu".into(), max_messages: 0, font_size: 999, ..Default::default() });
        assert_eq!((s.theme.as_str(), s.max_messages, s.font_size), ("orbite", 1, 96));
    }
}
