//! Connecteurs de chat. Pour ajouter une plateforme : implémenter
//! [`ChatConnector`] puis l'enregistrer dans [`ConnectorRegistry`].

mod twitch;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::domain::{Channel, ChatMessage, ModerationAction, Platform, StreamEvent, StreamInfo};
use crate::{Error, Result};

pub use twitch::{TokenHook, TwitchConnector, TwitchCreds};

/// Événements remontés par un connecteur vers l'agrégateur de chat.
#[derive(Debug, Clone)]
pub enum ChatEvent {
    Message(ChatMessage),
    Event(StreamEvent),
    MessageDeleted { channel_id: String, message_id: String },
    Connected { channel_id: String },
    Disconnected { channel_id: String },
}

#[async_trait]
pub trait ChatConnector: Send + Sync {
    fn platform(&self) -> Platform;

    /// Démarre l'écoute du chat de `channel`, les événements sont poussés dans `tx`.
    async fn connect(&self, channel: &Channel, tx: mpsc::Sender<ChatEvent>) -> Result<()>;

    async fn disconnect(&self, channel: &Channel) -> Result<()>;

    async fn send_message(&self, channel: &Channel, text: &str) -> Result<()>;

    /// Applique une action de modération native à la plateforme.
    async fn moderate(&self, action: &ModerationAction) -> Result<()>;

    /// État du direct (titre, spectateurs) ; `None` si la chaîne n'est pas en live.
    async fn stream_info(&self, _channel: &Channel) -> Result<Option<StreamInfo>> {
        Ok(None)
    }
}

#[derive(Default, Clone)]
pub struct ConnectorRegistry {
    connectors: HashMap<Platform, Arc<dyn ChatConnector>>,
}

impl ConnectorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, connector: Arc<dyn ChatConnector>) {
        self.connectors.insert(connector.platform(), connector);
    }

    pub fn get(&self, platform: &Platform) -> Result<Arc<dyn ChatConnector>> {
        self.connectors
            .get(platform)
            .cloned()
            .ok_or_else(|| Error::UnknownPlatform(platform.clone()))
    }

    pub fn platforms(&self) -> Vec<Platform> {
        self.connectors.keys().cloned().collect()
    }
}
