//! Connecteurs de chat. Pour ajouter une plateforme : implémenter
//! [`ChatConnector`] puis l'enregistrer dans [`ConnectorRegistry`].

mod twitch;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::domain::{
    Category, Channel, ChatMessage, ModerationAction, Platform, PlatformUpdateResult, StreamEvent,
    StreamInfo, StreamInfoUpdate,
};
use crate::{Error, Result};

pub use twitch::{TokenHook, TwitchConnector, TwitchCreds};

/// Événements remontés par un connecteur vers l'agrégateur de chat.
#[derive(Debug, Clone)]
pub enum ChatEvent {
    Message(ChatMessage),
    Event(StreamEvent),
    MessageDeleted {
        channel_id: String,
        message_id: String,
    },
    Connected {
        channel_id: String,
    },
    Disconnected {
        channel_id: String,
    },
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

    /// Modifie titre / catégorie / tags du direct. Par défaut : non supporté.
    async fn update_stream_info(
        &self,
        _channel: &Channel,
        _update: &StreamInfoUpdate,
    ) -> Result<()> {
        Err(Error::Unsupported {
            platform: self.platform(),
            feature: "la mise à jour du titre et de la catégorie",
        })
    }

    /// Recherche de catégories par texte libre. Par défaut : aucune suggestion.
    async fn search_categories(&self, _query: &str) -> Result<Vec<Category>> {
        Ok(Vec::new())
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

    /// Applique `update` à toutes les chaînes ; un échec n'empêche pas les autres plateformes.
    pub async fn update_stream_info(
        &self,
        channels: &[Channel],
        update: &StreamInfoUpdate,
    ) -> Vec<PlatformUpdateResult> {
        let tasks = channels.iter().map(|channel| async move {
            let outcome = match self.get(&channel.platform) {
                Ok(c) => c.update_stream_info(channel, update).await,
                Err(e) => Err(e),
            };
            PlatformUpdateResult {
                channel_id: channel.id.clone(),
                platform: channel.platform.clone(),
                ok: outcome.is_ok(),
                error: outcome.err().map(|e| e.to_string()),
            }
        });
        futures_util::future::join_all(tasks).await
    }

    pub fn platforms(&self) -> Vec<Platform> {
        self.connectors.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ok1;
    struct Default1;

    #[async_trait]
    impl ChatConnector for Ok1 {
        fn platform(&self) -> Platform {
            Platform::Twitch
        }
        async fn connect(&self, _: &Channel, _: mpsc::Sender<ChatEvent>) -> Result<()> {
            Ok(())
        }
        async fn disconnect(&self, _: &Channel) -> Result<()> {
            Ok(())
        }
        async fn send_message(&self, _: &Channel, _: &str) -> Result<()> {
            Ok(())
        }
        async fn moderate(&self, _: &ModerationAction) -> Result<()> {
            Ok(())
        }
        async fn update_stream_info(&self, _: &Channel, _: &StreamInfoUpdate) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl ChatConnector for Default1 {
        fn platform(&self) -> Platform {
            Platform::Kick
        }
        async fn connect(&self, _: &Channel, _: mpsc::Sender<ChatEvent>) -> Result<()> {
            Ok(())
        }
        async fn disconnect(&self, _: &Channel) -> Result<()> {
            Ok(())
        }
        async fn send_message(&self, _: &Channel, _: &str) -> Result<()> {
            Ok(())
        }
        async fn moderate(&self, _: &ModerationAction) -> Result<()> {
            Ok(())
        }
    }

    fn chan(p: Platform, id: &str) -> Channel {
        Channel {
            id: id.into(),
            platform: p,
            external_id: id.into(),
            display_name: id.into(),
        }
    }

    #[tokio::test]
    async fn partial_results_per_platform() {
        let mut reg = ConnectorRegistry::new();
        reg.register(Arc::new(Ok1));
        reg.register(Arc::new(Default1));
        let channels = [
            chan(Platform::Twitch, "t"),
            chan(Platform::Kick, "k"),
            chan(Platform::Youtube, "y"),
        ];
        let update = StreamInfoUpdate {
            title: Some("Hello".into()),
            ..Default::default()
        };
        let res = reg.update_stream_info(&channels, &update).await;
        assert_eq!(res.len(), 3);
        assert!(res[0].ok && res[0].error.is_none());
        assert!(!res[1].ok && res[1].error.as_deref().unwrap().contains("kick"));
        assert!(!res[2].ok);
    }
}
