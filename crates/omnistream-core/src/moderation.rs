//! Moteur de modération : une seule entrée, dispatch vers le bon connecteur.

use crate::connectors::ConnectorRegistry;
use crate::domain::ModerationAction;
use crate::Result;

pub struct ModerationEngine {
    connectors: ConnectorRegistry,
}

impl ModerationEngine {
    pub fn new(connectors: ConnectorRegistry) -> Self {
        Self { connectors }
    }

    pub async fn apply(&self, action: &ModerationAction) -> Result<()> {
        self.connectors
            .get(&action.channel.platform)?
            .moderate(action)
            .await
    }

    /// Applique la même action sur plusieurs chaînes (ex. ban global) ;
    /// retourne un résultat par chaîne, une erreur n'interrompt pas les autres.
    pub async fn apply_all(&self, actions: &[ModerationAction]) -> Vec<Result<()>> {
        let mut results = Vec::with_capacity(actions.len());
        for action in actions {
            results.push(self.apply(action).await);
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use super::*;
    use crate::connectors::{ChatConnector, ChatEvent};
    use crate::domain::{Channel, ModerationKind, Platform};
    use crate::Error;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<String>>);

    #[async_trait]
    impl ChatConnector for Recorder {
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
        async fn moderate(&self, a: &ModerationAction) -> Result<()> {
            self.0.lock().unwrap().push(a.kind.name().to_string());
            Ok(())
        }
    }

    fn action(platform: Platform) -> ModerationAction {
        ModerationAction {
            channel: Channel {
                id: "c1".into(),
                platform,
                external_id: "chan".into(),
                display_name: "Chan".into(),
            },
            kind: ModerationKind::Timeout { user_id: "u".into(), seconds: 60 },
        }
    }

    #[tokio::test]
    async fn dispatches_to_matching_connector() {
        let recorder = Arc::new(Recorder::default());
        let mut registry = ConnectorRegistry::new();
        registry.register(recorder.clone());
        let engine = ModerationEngine::new(registry);

        engine.apply(&action(Platform::Kick)).await.unwrap();
        assert_eq!(*recorder.0.lock().unwrap(), vec!["timeout"]);

        let err = engine.apply(&action(Platform::Twitch)).await.unwrap_err();
        assert!(matches!(err, Error::UnknownPlatform(Platform::Twitch)));
    }
}
