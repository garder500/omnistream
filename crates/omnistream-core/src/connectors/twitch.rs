use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{ChatConnector, ChatEvent};
use crate::domain::{Channel, ModerationAction, Platform};
use crate::{Error, Result};

/// Squelette : la connexion IRC/WebSocket Twitch reste à écrire.
pub struct TwitchConnector;

#[async_trait]
impl ChatConnector for TwitchConnector {
    fn platform(&self) -> Platform {
        Platform::Twitch
    }

    async fn connect(&self, _channel: &Channel, _tx: mpsc::Sender<ChatEvent>) -> Result<()> {
        Err(Error::NotImplemented("TwitchConnector::connect"))
    }

    async fn disconnect(&self, _channel: &Channel) -> Result<()> {
        Err(Error::NotImplemented("TwitchConnector::disconnect"))
    }

    async fn send_message(&self, _channel: &Channel, _text: &str) -> Result<()> {
        Err(Error::NotImplemented("TwitchConnector::send_message"))
    }

    async fn moderate(&self, _action: &ModerationAction) -> Result<()> {
        Err(Error::NotImplemented("TwitchConnector::moderate"))
    }
}
