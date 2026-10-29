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

/// Détection simple (sans IA) des messages à relire : liens, majuscules, répétitions.
pub fn flag_reason(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    if ["http://", "https://", "www.", "bit.ly/", ".ly/"]
        .iter()
        .any(|p| lower.contains(p))
    {
        return Some("Lien suspect");
    }
    let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() >= 12
        && letters.iter().filter(|c| c.is_uppercase()).count() * 10 >= letters.len() * 8
    {
        return Some("Majuscules");
    }
    let words: Vec<&str> = lower.split_whitespace().collect();
    if words.len() >= 4 && words.iter().all(|w| *w == words[0]) {
        return Some("Répétition");
    }
    let mut run = 1;
    let mut prev = None;
    for c in text.chars() {
        run = if Some(c) == prev { run + 1 } else { 1 };
        if run >= 15 {
            return Some("Répétition");
        }
        prev = Some(c);
    }
    None
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

    #[test]
    fn flags_spam_but_not_normal_chat() {
        assert_eq!(
            flag_reason("achetez des abonnés sur bit.ly/xxxx"),
            Some("Lien suspect")
        );
        assert_eq!(
            flag_reason("REGARDEZ MA CHAINE REGARDEZ MA CHAINE"),
            Some("Majuscules")
        );
        assert_eq!(flag_reason("spam spam spam spam"), Some("Répétition"));
        assert_eq!(flag_reason("aaaaaaaaaaaaaaaaaaaa"), Some("Répétition"));
        assert_eq!(flag_reason("GG pour le boss !!"), None);
        assert_eq!(flag_reason("LOL"), None);
    }
}
