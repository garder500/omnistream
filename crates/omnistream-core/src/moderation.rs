//! Moteur de modération : une seule entrée, dispatch vers le bon connecteur.

use crate::automod::Verdict;
use crate::connectors::ConnectorRegistry;
use crate::domain::{Channel, ChatMessage, ModerationAction};
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

    /// Applique les actions décidées par l'automod (timeout, suppression) sur la
    /// chaîne du message, quelle que soit sa plateforme. Un échec n'interrompt
    /// pas les autres actions.
    pub async fn apply_verdict(
        &self,
        channel: &Channel,
        msg: &ChatMessage,
        verdict: &Verdict,
    ) -> Vec<Result<()>> {
        let actions: Vec<ModerationAction> = verdict
            .moderation_kinds(msg)
            .into_iter()
            .map(|kind| ModerationAction {
                channel: channel.clone(),
                kind,
            })
            .collect();
        self.apply_all(&actions).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use super::*;
    use crate::automod::{AutoMod, Rule, RuleAction, RuleKind};
    use crate::connectors::{ChatConnector, ChatEvent};
    use crate::domain::{Channel, ChatAuthor, ModerationKind, Platform};
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
            kind: ModerationKind::Timeout {
                user_id: "u".into(),
                seconds: 60,
            },
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

    fn chat(text: &str) -> ChatMessage {
        ChatMessage {
            id: "m1".into(),
            channel_id: "c1".into(),
            platform: Platform::Kick,
            author: ChatAuthor {
                external_id: "u9".into(),
                display_name: "Bob".into(),
                is_moderator: false,
                is_subscriber: false,
            },
            text: text.into(),
            timestamp_ms: 0,
            emotes: vec![],
            badges: vec![],
            room_id: None,
        }
    }

    fn rule(id: &str, kind: RuleKind, action: RuleAction) -> Rule {
        Rule {
            id: id.into(),
            name: id.into(),
            enabled: true,
            score: 10,
            kind,
            action,
            platforms: vec![],
        }
    }

    #[tokio::test]
    async fn applies_automod_verdict_through_the_connector() {
        let recorder = Arc::new(Recorder::default());
        let mut registry = ConnectorRegistry::new();
        registry.register(recorder.clone());
        let engine = ModerationEngine::new(registry);
        let channel = action(Platform::Kick).channel;

        let mut automod = AutoMod::new(vec![
            rule(
                "liens",
                RuleKind::Links {
                    allowed_domains: vec![],
                },
                RuleAction::Delete,
            ),
            rule(
                "mots",
                RuleKind::BannedWords {
                    words: vec!["arnaque".into()],
                },
                RuleAction::Timeout { seconds: 300 },
            ),
            rule(
                "caps",
                RuleKind::Caps {
                    min_letters: 3,
                    ratio_percent: 80,
                },
                RuleAction::Flag,
            ),
        ]);

        let m = chat("arnaque sur bit.ly/x");
        let verdict = automod.evaluate(&m);
        let results = engine.apply_verdict(&channel, &m, &verdict).await;
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(
            *recorder.0.lock().unwrap(),
            vec!["timeout", "delete_message"]
        );

        // flag seul : aucune action de modération
        recorder.0.lock().unwrap().clear();
        let m = chat("BONJOUR TOUT LE MONDE");
        let verdict = automod.evaluate(&m);
        assert!(!verdict.is_clean());
        assert!(engine
            .apply_verdict(&channel, &m, &verdict)
            .await
            .is_empty());
        assert!(recorder.0.lock().unwrap().is_empty());

        // plateforme sans connecteur : l'erreur est remontée sans paniquer
        let m = chat("arnaque");
        let verdict = automod.evaluate(&m);
        let mut twitch = channel.clone();
        twitch.platform = Platform::Twitch;
        let results = engine.apply_verdict(&twitch, &m, &verdict).await;
        assert!(matches!(
            results[0],
            Err(Error::UnknownPlatform(Platform::Twitch))
        ));
    }
}
