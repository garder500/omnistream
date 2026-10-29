//! Exécution des automatisations : relie le moteur pur ([`crate::automation`]) à la base locale
//! et aux connecteurs de chat. Les effets sont exécutés ici, jamais dans le moteur.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::automation::{AutoEvent, Clock, Effect, Engine, Scope};
use crate::connectors::{ChatEvent, ConnectorRegistry};
use crate::db::Database;
use crate::domain::{Channel, ModerationAction};
use crate::Result;

const MAX_LOG: usize = 100;
/// Fenêtre pendant laquelle un message identique à un envoi récent est considéré comme l'écho
/// de notre propre message (évite les boucles si une plateforme renvoie les messages du bot).
const ECHO_WINDOW_MS: i64 = 15_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LogEntry {
    pub at_ms: i64,
    /// Nom de la règle ou `!commande` à l'origine.
    pub origin: String,
    pub summary: String,
    pub error: Option<String>,
}

pub struct AutomationService {
    db: Arc<Database>,
    connectors: ConnectorRegistry,
    clock: Arc<dyn Clock>,
    engine: Mutex<Engine>,
    log: Mutex<VecDeque<LogEntry>>,
    /// (chaîne, texte, instant) des derniers messages envoyés.
    sent: Mutex<Vec<(String, String, i64)>>,
}

impl AutomationService {
    pub fn new(db: Arc<Database>, connectors: ConnectorRegistry, clock: Arc<dyn Clock>) -> Result<Self> {
        let svc = Self {
            db,
            connectors,
            engine: Mutex::new(Engine::new(clock.clone())),
            clock,
            log: Mutex::new(VecDeque::new()),
            sent: Mutex::new(Vec::new()),
        };
        svc.reload()?;
        Ok(svc)
    }

    /// Relit règles, commandes et chaînes depuis la base (à appeler après chaque modification).
    pub fn reload(&self) -> Result<()> {
        let rules = self.db.list_rules()?;
        let commands = self.db.list_commands()?;
        let channels = self.db.list_channels()?;
        let mut engine = self.engine.lock().unwrap();
        engine.set_channels(channels);
        engine.set_commands(commands);
        engine.set_rules(rules);
        Ok(())
    }

    pub fn recent_log(&self) -> Vec<LogEntry> {
        self.log.lock().unwrap().iter().cloned().collect()
    }

    /// Hook appelé pour chaque événement entrant des connecteurs.
    pub async fn on_chat_event(&self, event: &ChatEvent) {
        let auto = match event {
            ChatEvent::Message(m) => {
                // Le bot écrit avec le compte du diffuseur : on n'ignore que ses propres échos.
                if self.engine.lock().unwrap().is_broadcaster(m) && self.is_echo(&m.channel_id, &m.text) {
                    return;
                }
                AutoEvent::Message(m.clone())
            }
            ChatEvent::Event(e) => AutoEvent::Stream(e.clone()),
            _ => return,
        };
        let effects = self.engine.lock().unwrap().handle(&auto);
        self.execute(effects).await;
    }

    /// À appeler environ chaque seconde : minuteurs.
    pub async fn tick(&self) {
        let effects = self.engine.lock().unwrap().tick();
        self.execute(effects).await;
    }

    /// À appeler périodiquement (≈ 1 min) : détecte début/fin de live.
    pub async fn poll_live(&self) {
        let channels = self.db.list_channels().unwrap_or_default();
        for channel in channels {
            let Ok(connector) = self.connectors.get(&channel.platform) else { continue };
            // En cas d'erreur réseau on ne conclut rien : pas de faux « fin de stream ».
            let Ok(info) = connector.stream_info(&channel).await else { continue };
            let effects = self.engine.lock().unwrap().observe_live(&channel.id, &channel.platform, info.is_some());
            self.execute(effects).await;
        }
    }

    fn is_echo(&self, channel_id: &str, text: &str) -> bool {
        let now = self.clock.now_ms();
        let mut sent = self.sent.lock().unwrap();
        sent.retain(|(_, _, at)| now - at <= ECHO_WINDOW_MS);
        sent.iter().any(|(c, t, _)| c == channel_id && t == text)
    }

    fn push_log(&self, origin: &str, summary: String, error: Option<String>) {
        let mut log = self.log.lock().unwrap();
        log.push_front(LogEntry { at_ms: self.clock.now_ms(), origin: origin.to_string(), summary, error });
        log.truncate(MAX_LOG);
    }

    pub async fn execute(&self, effects: Vec<Effect>) {
        if effects.is_empty() {
            return;
        }
        let channels = self.db.list_channels().unwrap_or_default();
        for effect in effects {
            match effect {
                Effect::Send { scope, text, origin } => {
                    let targets: Vec<&Channel> = match &scope {
                        Scope::Channel(id) => channels.iter().filter(|c| &c.id == id).collect(),
                        Scope::AllChannels => channels.iter().collect(),
                        Scope::Platforms(ps) => channels.iter().filter(|c| ps.contains(&c.platform)).collect(),
                    };
                    if targets.is_empty() {
                        self.push_log(&origin, format!("« {text} » : aucune chaîne cible"), Some("aucune chaîne cible".into()));
                    }
                    for channel in targets {
                        let res = match self.connectors.get(&channel.platform) {
                            Ok(c) => c.send_message(channel, &text).await,
                            Err(e) => Err(e),
                        };
                        if res.is_ok() {
                            self.sent.lock().unwrap().push((channel.id.clone(), text.clone(), self.clock.now_ms()));
                        }
                        self.push_log(
                            &origin,
                            format!("Message sur {} : {text}", channel.display_name),
                            res.err().map(|e| e.to_string()),
                        );
                    }
                }
                Effect::Moderate { channel_id, kind, origin } => {
                    let summary = format!("Modération ({}) sur {channel_id}", kind.name());
                    let res = match channels.iter().find(|c| c.id == channel_id) {
                        Some(channel) => match self.connectors.get(&channel.platform) {
                            Ok(c) => c.moderate(&ModerationAction { channel: channel.clone(), kind }).await,
                            Err(e) => Err(e),
                        },
                        None => Err(crate::Error::Other(format!("chaîne inconnue: {channel_id}"))),
                    };
                    self.push_log(&origin, summary, res.err().map(|e| e.to_string()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{Action, ChatCommand, ManualClock, Role, Rule, Target, Trigger};
    use crate::connectors::ChatConnector;
    use crate::domain::{ChatAuthor, ChatMessage, Platform, StreamEvent, StreamEventKind, StreamInfo};
    use async_trait::async_trait;
    use tokio::sync::mpsc;

    struct Mock {
        platform: Platform,
        sent: Mutex<Vec<(String, String)>>,
        moderated: Mutex<Vec<String>>,
        live: Mutex<Option<bool>>, // None = erreur réseau
    }

    impl Mock {
        fn new(platform: Platform) -> Arc<Self> {
            Arc::new(Self { platform, sent: Mutex::default(), moderated: Mutex::default(), live: Mutex::new(Some(false)) })
        }
    }

    #[async_trait]
    impl ChatConnector for Mock {
        fn platform(&self) -> Platform {
            self.platform.clone()
        }
        async fn connect(&self, _: &Channel, _: mpsc::Sender<ChatEvent>) -> Result<()> {
            Ok(())
        }
        async fn disconnect(&self, _: &Channel) -> Result<()> {
            Ok(())
        }
        async fn send_message(&self, c: &Channel, text: &str) -> Result<()> {
            self.sent.lock().unwrap().push((c.id.clone(), text.to_string()));
            Ok(())
        }
        async fn moderate(&self, a: &ModerationAction) -> Result<()> {
            self.moderated.lock().unwrap().push(a.kind.name().to_string());
            Ok(())
        }
        async fn stream_info(&self, c: &Channel) -> Result<Option<StreamInfo>> {
            match *self.live.lock().unwrap() {
                None => Err(crate::Error::Network("hors ligne".into())),
                Some(false) => Ok(None),
                Some(true) => Ok(Some(StreamInfo { channel_id: c.id.clone(), title: "t".into(), viewers: 1, started_at: "".into() })),
            }
        }
    }

    fn setup() -> (AutomationService, Arc<Mock>, Arc<Mock>, Arc<ManualClock>, Arc<Database>) {
        let db = Arc::new(Database::open_in_memory().unwrap());
        for (p, id) in [(Platform::Twitch, "twitch:g"), (Platform::Kick, "kick:g")] {
            db.save_channel(&Channel { id: id.into(), platform: p, external_id: "g".into(), display_name: "g".into() }).unwrap();
        }
        let tw = Mock::new(Platform::Twitch);
        let kick = Mock::new(Platform::Kick);
        let mut reg = ConnectorRegistry::new();
        reg.register(tw.clone());
        reg.register(kick.clone());
        let clock = ManualClock::new(1_000_000);
        let svc = AutomationService::new(db.clone(), reg, clock.clone()).unwrap();
        (svc, tw, kick, clock, db)
    }

    fn chat(platform: Platform, text: &str) -> ChatEvent {
        chat_as(platform, text, "Alice")
    }

    fn chat_as(platform: Platform, text: &str, who: &str) -> ChatEvent {
        let id = format!("{platform}:g");
        ChatEvent::Message(ChatMessage {
            id: "m".into(),
            channel_id: id,
            platform,
            author: ChatAuthor { external_id: format!("id-{who}"), display_name: who.into(), is_moderator: false, is_subscriber: false },
            text: text.into(),
            timestamp_ms: 0,
        })
    }

    fn command(name: &str, response: &str, reply: Target) -> ChatCommand {
        ChatCommand {
            id: name.into(),
            name: name.into(),
            response: response.into(),
            enabled: true,
            cooldown_secs: 0,
            user_cooldown_secs: 0,
            min_role: Role::Everyone,
            platforms: vec![],
            reply,
        }
    }

    #[tokio::test]
    async fn command_from_kick_replies_on_kick_only() {
        let (svc, tw, kick, _, db) = setup();
        db.save_command(&command("hi", "salut {user}", Target::Source)).unwrap();
        svc.reload().unwrap();
        svc.on_chat_event(&chat(Platform::Kick, "!hi")).await;
        assert_eq!(*kick.sent.lock().unwrap(), vec![("kick:g".to_string(), "salut Alice".to_string())]);
        assert!(tw.sent.lock().unwrap().is_empty());
        assert_eq!(svc.recent_log().len(), 1);
    }

    #[tokio::test]
    async fn command_reply_to_all_platforms() {
        let (svc, tw, kick, _, db) = setup();
        db.save_command(&command("all", "x", Target::All)).unwrap();
        svc.reload().unwrap();
        svc.on_chat_event(&chat(Platform::Twitch, "!all")).await;
        assert_eq!(tw.sent.lock().unwrap().len(), 1);
        assert_eq!(kick.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn own_replies_echoed_back_are_ignored_for_a_while() {
        let (svc, tw, _, clock, db) = setup();
        db.save_command(&command("hi", "!hi", Target::Source)).unwrap(); // pire cas : la réponse est elle-même une commande
        svc.reload().unwrap();
        svc.on_chat_event(&chat(Platform::Twitch, "!hi")).await;
        assert_eq!(tw.sent.lock().unwrap().len(), 1);
        // Un spectateur qui recopie le texte du bot n'est pas un écho.
        svc.on_chat_event(&chat(Platform::Twitch, "!hi")).await;
        assert_eq!(tw.sent.lock().unwrap().len(), 2);
        // Le compte du diffuseur (= le bot) qui revoit son propre message : ignoré, pas de boucle.
        svc.on_chat_event(&chat_as(Platform::Twitch, "!hi", "g")).await;
        assert_eq!(tw.sent.lock().unwrap().len(), 2, "écho ignoré, pas de boucle");
        clock.advance_secs(16);
        svc.on_chat_event(&chat_as(Platform::Twitch, "!hi", "g")).await;
        assert_eq!(tw.sent.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn moderation_rule_is_applied_through_connector() {
        let (svc, tw, _, _, db) = setup();
        db.save_rule(&Rule {
            id: "r".into(),
            name: "anti-spam".into(),
            enabled: true,
            trigger: Trigger::Message { contains: "spam".into() },
            action: Action::Timeout { seconds: 60 },
            platforms: vec![],
            cooldown_secs: 0,
        })
        .unwrap();
        svc.reload().unwrap();
        svc.on_chat_event(&chat(Platform::Twitch, "du SPAM")).await;
        assert_eq!(*tw.moderated.lock().unwrap(), vec!["timeout".to_string()]);
    }

    #[tokio::test]
    async fn stream_event_and_timer_and_live_transitions() {
        let (svc, tw, kick, clock, db) = setup();
        let say = |id: &str, trigger, target| Rule {
            id: id.into(),
            name: id.into(),
            enabled: true,
            trigger,
            action: Action::SendMessage { target, text: id.into() },
            platforms: vec![],
            cooldown_secs: 0,
        };
        db.save_rule(&say("follow", Trigger::Follow, Target::Source)).unwrap();
        db.save_rule(&say("tic", Trigger::Timer { every_secs: 30 }, Target::All)).unwrap();
        db.save_rule(&say("live", Trigger::StreamStart, Target::All)).unwrap();
        svc.reload().unwrap();

        svc.on_chat_event(&ChatEvent::Event(StreamEvent {
            id: "e".into(),
            channel_id: "kick:g".into(),
            platform: Platform::Kick,
            who: "Bob".into(),
            kind: StreamEventKind::Follow,
            message: None,
            timestamp_ms: 0,
        }))
        .await;
        assert_eq!(kick.sent.lock().unwrap().len(), 1);
        assert_eq!(tw.sent.lock().unwrap().len(), 0);

        svc.tick().await;
        assert_eq!(tw.sent.lock().unwrap().len(), 0);
        clock.advance_secs(30);
        svc.tick().await;
        assert_eq!(tw.sent.lock().unwrap().len(), 1);

        // hors ligne -> erreur réseau (ignorée) -> en ligne
        svc.poll_live().await;
        *tw.live.lock().unwrap() = None;
        svc.poll_live().await;
        *tw.live.lock().unwrap() = Some(true);
        svc.poll_live().await;
        let live_msgs = |m: &Arc<Mock>| m.sent.lock().unwrap().iter().filter(|(_, t)| t == "live").count();
        assert_eq!(live_msgs(&tw), 1, "le début de live est envoyé sur toutes les chaînes");
        assert_eq!(live_msgs(&kick), 1);
        svc.poll_live().await;
        assert_eq!(live_msgs(&kick), 1, "pas de nouveau déclenchement tant que le live continue");
    }

    #[tokio::test]
    async fn missing_target_is_logged_not_fatal() {
        let (svc, _, _, _, _) = setup();
        svc.execute(vec![Effect::Send { scope: Scope::Channel("nope".into()), text: "x".into(), origin: "o".into() }]).await;
        let log = svc.recent_log();
        assert_eq!(log.len(), 1);
        assert!(log[0].error.is_some());
    }
}
