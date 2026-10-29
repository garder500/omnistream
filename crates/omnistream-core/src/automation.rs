//! Moteur d'automatisation local : règles « si X alors Y » et commandes de chat `!cmd`.
//!
//! Le moteur est **pur** : aucune E/S, aucun `await`. Il reçoit des [`AutoEvent`], consulte
//! une [`Clock`] injectable (cooldowns, minuteurs) et renvoie des [`Effect`] que l'appelant
//! (voir `automation_service`) exécute via les connecteurs. Cela le rend testable à 100 %.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::{
    Channel, ChatAuthor, ChatMessage, ModerationKind, Platform, StreamEvent, StreamEventKind,
};
use crate::{Error, Result};

// ---------------------------------------------------------------------------------------------
// Horloge
// ---------------------------------------------------------------------------------------------

/// Source de temps injectable (millisecondes depuis l'epoch Unix).
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

/// Horloge manuelle pour les tests : le temps n'avance que via [`ManualClock::advance_ms`].
#[derive(Debug, Default)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    pub fn new(start_ms: i64) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(start_ms)))
    }
    pub fn advance_ms(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
    pub fn advance_secs(&self, s: i64) {
        self.advance_ms(s * 1000);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------------------------
// Modèle
// ---------------------------------------------------------------------------------------------

/// Rôle d'un auteur, du moins au plus privilégié.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    Everyone,
    Subscriber,
    Moderator,
    Broadcaster,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Everyone => "everyone",
            Role::Subscriber => "subscriber",
            Role::Moderator => "moderator",
            Role::Broadcaster => "broadcaster",
        }
    }
}

impl From<&str> for Role {
    fn from(s: &str) -> Self {
        match s {
            "subscriber" => Role::Subscriber,
            "moderator" => Role::Moderator,
            "broadcaster" => Role::Broadcaster,
            _ => Role::Everyone,
        }
    }
}

/// Rôle effectif d'un auteur sur une chaîne. Le diffuseur est reconnu à son pseudo (= identifiant
/// de la chaîne) ou à son identifiant plateforme.
pub fn role_of(author: &ChatAuthor, channel: Option<&Channel>) -> Role {
    if let Some(c) = channel {
        if author.display_name.eq_ignore_ascii_case(&c.external_id)
            || author.external_id == c.external_id
        {
            return Role::Broadcaster;
        }
    }
    if author.is_moderator {
        Role::Moderator
    } else if author.is_subscriber {
        Role::Subscriber
    } else {
        Role::Everyone
    }
}

/// Où envoyer un message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// La chaîne qui a déclenché la règle (toutes les chaînes si elle n'en a pas, ex. minuteur).
    #[default]
    Source,
    /// Toutes les chaînes suivies, sur toutes les plateformes.
    All,
    /// Toutes les chaînes des plateformes listées.
    Platforms { platforms: Vec<Platform> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Message de chat ; `contains` vide = tous les messages (insensible à la casse).
    Message {
        #[serde(default)]
        contains: String,
    },
    Follow,
    Sub,
    Raid {
        #[serde(default)]
        min_viewers: u32,
    },
    /// Toutes les `every_secs` secondes (≥ 1).
    Timer {
        every_secs: u64,
    },
    StreamStart,
    StreamEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    SendMessage {
        #[serde(default)]
        target: Target,
        text: String,
    },
    /// Les trois suivantes visent l'auteur du message déclencheur (déclencheur `message` requis).
    Timeout {
        seconds: u32,
    },
    DeleteMessage,
    Ban {
        #[serde(default)]
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub action: Action,
    /// Plateformes concernées ; vide = toutes.
    #[serde(default)]
    pub platforms: Vec<Platform>,
    /// Délai minimal entre deux exécutions.
    #[serde(default)]
    pub cooldown_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCommand {
    pub id: String,
    /// Nom sans le `!`, en minuscules (ex. `discord`).
    pub name: String,
    pub response: String,
    pub enabled: bool,
    /// Délai global entre deux utilisations.
    #[serde(default)]
    pub cooldown_secs: u32,
    /// Délai par utilisateur.
    #[serde(default)]
    pub user_cooldown_secs: u32,
    #[serde(default)]
    pub min_role: Role,
    /// Plateformes où la commande répond ; vide = toutes.
    #[serde(default)]
    pub platforms: Vec<Platform>,
    /// Où publier la réponse (par défaut : là où la commande a été tapée).
    #[serde(default)]
    pub reply: Target,
}

impl Rule {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(Error::Other("règle sans identifiant".into()));
        }
        if self.name.trim().is_empty() {
            return Err(Error::Other("la règle doit avoir un nom".into()));
        }
        if let Trigger::Timer { every_secs } = self.trigger {
            if every_secs == 0 {
                return Err(Error::Other(
                    "le minuteur doit durer au moins 1 seconde".into(),
                ));
            }
        }
        match &self.action {
            Action::SendMessage { text, .. } if text.trim().is_empty() => {
                return Err(Error::Other("le message à envoyer est vide".into()));
            }
            Action::Timeout { .. } | Action::DeleteMessage | Action::Ban { .. }
                if !matches!(self.trigger, Trigger::Message { .. }) =>
            {
                return Err(Error::Other(
                    "la modération n'est possible qu'avec le déclencheur « message »".into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }
}

impl ChatCommand {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(Error::Other("commande sans identifiant".into()));
        }
        if self.name.is_empty() || !self.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(Error::Other(
                "nom de commande invalide (lettres, chiffres et _ uniquement)".into(),
            ));
        }
        if self.response.trim().is_empty() {
            return Err(Error::Other("la réponse de la commande est vide".into()));
        }
        Ok(())
    }

    /// Normalise le nom saisi par l'utilisateur (`!Discord` -> `discord`).
    pub fn normalize_name(raw: &str) -> String {
        raw.trim().trim_start_matches('!').to_lowercase()
    }
}

// ---------------------------------------------------------------------------------------------
// Événements et effets
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AutoEvent {
    Message(ChatMessage),
    /// Abonnement, don de subs, raid ou follow.
    Stream(StreamEvent),
    StreamStarted {
        channel_id: String,
        platform: Platform,
    },
    StreamEnded {
        channel_id: String,
        platform: Platform,
    },
}

/// Portée d'un envoi, déjà résolue par le moteur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Channel(String),
    AllChannels,
    Platforms(Vec<Platform>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Send {
        scope: Scope,
        text: String,
        origin: String,
    },
    Moderate {
        channel_id: String,
        kind: ModerationKind,
        origin: String,
    },
}

// ---------------------------------------------------------------------------------------------
// Gabarits
// ---------------------------------------------------------------------------------------------

/// Remplace les `{variable}` connues ; les inconnues sont laissées telles quelles.
pub fn render(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Découpe `!name arg1 arg2` en (nom en minuscules, arguments). `None` si ce n'est pas une commande.
pub fn parse_command(text: &str) -> Option<(String, String)> {
    let t = text.trim();
    let body = t.strip_prefix('!')?;
    let mut it = body.splitn(2, char::is_whitespace);
    let name = it.next()?.to_lowercase();
    if name.is_empty() {
        return None;
    }
    Some((name, it.next().unwrap_or("").trim().to_string()))
}

// ---------------------------------------------------------------------------------------------
// Moteur
// ---------------------------------------------------------------------------------------------

pub struct Engine {
    clock: Arc<dyn Clock>,
    rules: Vec<Rule>,
    commands: Vec<ChatCommand>,
    channels: Vec<Channel>,
    /// Dernier déclenchement par clé (`rule:<id>` / `cmd:<id>`).
    last_fired: HashMap<String, i64>,
    /// Dernier usage par (commande, auteur).
    last_user: HashMap<(String, String), i64>,
    /// (intervalle, prochaine échéance) des minuteurs, par identifiant de règle.
    timers: HashMap<String, (u64, i64)>,
    /// Dernier état de live connu, par chaîne.
    live: HashMap<String, bool>,
}

impl Engine {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            rules: vec![],
            commands: vec![],
            channels: vec![],
            last_fired: HashMap::new(),
            last_user: HashMap::new(),
            timers: HashMap::new(),
            live: HashMap::new(),
        }
    }

    pub fn set_channels(&mut self, channels: Vec<Channel>) {
        self.channels = channels;
    }

    pub fn set_commands(&mut self, commands: Vec<ChatCommand>) {
        self.commands = commands;
    }

    /// Remplace les règles. Les minuteurs inchangés gardent leur échéance, les nouveaux
    /// démarrent maintenant (premier déclenchement après un intervalle complet).
    pub fn set_rules(&mut self, rules: Vec<Rule>) {
        let now = self.clock.now_ms();
        let mut timers = HashMap::new();
        for r in &rules {
            if let Trigger::Timer { every_secs } = r.trigger {
                let due = match self.timers.get(&r.id) {
                    Some((prev, due)) if *prev == every_secs => *due,
                    _ => now + every_secs as i64 * 1000,
                };
                timers.insert(r.id.clone(), (every_secs, due));
            }
        }
        self.timers = timers;
        self.rules = rules;
    }

    pub fn handle(&mut self, event: &AutoEvent) -> Vec<Effect> {
        let mut effects = Vec::new();
        if let AutoEvent::Message(m) = event {
            self.run_commands(m, &mut effects);
        }
        self.run_rules(event, &mut effects);
        effects
    }

    /// À appeler régulièrement (≈ 1 s) : déclenche les minuteurs échus.
    pub fn tick(&mut self) -> Vec<Effect> {
        let now = self.clock.now_ms();
        let mut effects = Vec::new();
        for rule in self.rules.iter().filter(|r| r.enabled) {
            let Some((every, due)) = self.timers.get_mut(&rule.id) else {
                continue;
            };
            if now < *due {
                continue;
            }
            // Pas de rattrapage en rafale après une longue pause : on repart de maintenant.
            *due = now + *every as i64 * 1000;
            if let Some(mut e) = Self::action_effect(rule, None, None, &HashMap::new()) {
                // Sans chaîne source, le filtre de plateformes de la règle restreint l'envoi.
                if let Effect::Send {
                    scope: scope @ Scope::AllChannels,
                    ..
                } = &mut e
                {
                    if !rule.platforms.is_empty() {
                        *scope = Scope::Platforms(rule.platforms.clone());
                    }
                }
                effects.push(e);
            }
        }
        effects
    }

    /// Enregistre l'état de live d'une chaîne et renvoie les effets des transitions
    /// hors-ligne -> en ligne (`StreamStart`) et en ligne -> hors-ligne (`StreamEnd`).
    /// La première observation d'une chaîne déjà en live ne déclenche rien.
    pub fn observe_live(
        &mut self,
        channel_id: &str,
        platform: &Platform,
        live: bool,
    ) -> Vec<Effect> {
        let prev = self.live.insert(channel_id.to_string(), live);
        let event = match (prev, live) {
            (Some(false), true) => AutoEvent::StreamStarted {
                channel_id: channel_id.into(),
                platform: platform.clone(),
            },
            (Some(true), false) => AutoEvent::StreamEnded {
                channel_id: channel_id.into(),
                platform: platform.clone(),
            },
            _ => return vec![],
        };
        self.handle(&event)
    }

    fn channel(&self, id: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    fn run_commands(&mut self, m: &ChatMessage, effects: &mut Vec<Effect>) {
        let Some((name, args)) = parse_command(&m.text) else {
            return;
        };
        let now = self.clock.now_ms();
        let role = role_of(&m.author, self.channel(&m.channel_id));
        let Some(cmd) = self.commands.iter().find(|c| c.enabled && c.name == name) else {
            return;
        };
        if !cmd.platforms.is_empty() && !cmd.platforms.contains(&m.platform) {
            return;
        }
        if role < cmd.min_role {
            return;
        }
        let key = format!("cmd:{}", cmd.id);
        if let Some(last) = self.last_fired.get(&key) {
            if now - last < cmd.cooldown_secs as i64 * 1000 {
                return;
            }
        }
        let ukey = (cmd.id.clone(), m.author.external_id.clone());
        if let Some(last) = self.last_user.get(&ukey) {
            if now - last < cmd.user_cooldown_secs as i64 * 1000 {
                return;
            }
        }
        let mut vars = message_vars(m, &args);
        vars.insert("command", cmd.name.clone());
        let text = render(&cmd.response, &vars);
        let scope = resolve_scope(&cmd.reply, Some(&m.channel_id));
        effects.push(Effect::Send {
            scope,
            text,
            origin: format!("!{}", cmd.name),
        });
        self.last_fired.insert(key, now);
        self.last_user.insert(ukey, now);
    }

    /// Vrai si l'auteur est le diffuseur de la chaîne du message.
    pub fn is_broadcaster(&self, m: &ChatMessage) -> bool {
        role_of(&m.author, self.channel(&m.channel_id)) == Role::Broadcaster
    }

    fn run_rules(&mut self, event: &AutoEvent, effects: &mut Vec<Effect>) {
        let now = self.clock.now_ms();
        // Le diffuseur et les modérateurs ne sont jamais ciblés par une règle de modération.
        let exempt = match event {
            AutoEvent::Message(m) => {
                role_of(&m.author, self.channel(&m.channel_id)) >= Role::Moderator
            }
            _ => false,
        };
        for rule in self.rules.iter().filter(|r| r.enabled) {
            if exempt && !matches!(rule.action, Action::SendMessage { .. }) {
                continue;
            }
            let Some((platform, channel_id, vars, msg)) = match_trigger(&rule.trigger, event)
            else {
                continue;
            };
            if !rule.platforms.is_empty() && !rule.platforms.contains(&platform) {
                continue;
            }
            let key = format!("rule:{}", rule.id);
            if let Some(last) = self.last_fired.get(&key) {
                if now - last < rule.cooldown_secs as i64 * 1000 {
                    continue;
                }
            }
            if let Some(e) = Self::action_effect(rule, Some(&channel_id), msg.as_ref(), &vars) {
                effects.push(e);
                self.last_fired.insert(key, now);
            }
        }
    }

    fn action_effect(
        rule: &Rule,
        channel_id: Option<&str>,
        msg: Option<&ChatMessage>,
        vars: &HashMap<&str, String>,
    ) -> Option<Effect> {
        let origin = rule.name.clone();
        match &rule.action {
            Action::SendMessage { target, text } => Some(Effect::Send {
                scope: resolve_scope(target, channel_id),
                text: render(text, vars),
                origin,
            }),
            Action::Timeout { seconds } => {
                let m = msg?;
                Some(Effect::Moderate {
                    channel_id: m.channel_id.clone(),
                    kind: ModerationKind::Timeout {
                        user_id: m.author.external_id.clone(),
                        seconds: *seconds,
                    },
                    origin,
                })
            }
            Action::DeleteMessage => {
                let m = msg?;
                Some(Effect::Moderate {
                    channel_id: m.channel_id.clone(),
                    kind: ModerationKind::DeleteMessage {
                        message_id: m.id.clone(),
                    },
                    origin,
                })
            }
            Action::Ban { reason } => {
                let m = msg?;
                Some(Effect::Moderate {
                    channel_id: m.channel_id.clone(),
                    kind: ModerationKind::Ban {
                        user_id: m.author.external_id.clone(),
                        reason: reason.clone(),
                    },
                    origin,
                })
            }
        }
    }
}

fn resolve_scope(target: &Target, source: Option<&str>) -> Scope {
    match target {
        Target::Source => source
            .map(|s| Scope::Channel(s.to_string()))
            .unwrap_or(Scope::AllChannels),
        Target::All => Scope::AllChannels,
        Target::Platforms { platforms } => Scope::Platforms(platforms.clone()),
    }
}

fn message_vars<'a>(m: &ChatMessage, args: &str) -> HashMap<&'a str, String> {
    let mut v = HashMap::new();
    v.insert("user", m.author.display_name.clone());
    v.insert("platform", m.platform.to_string());
    v.insert("args", args.to_string());
    v.insert("message", m.text.clone());
    const NAMES: [&str; 5] = ["arg1", "arg2", "arg3", "arg4", "arg5"];
    for (name, a) in NAMES.iter().zip(args.split_whitespace()) {
        v.insert(name, a.to_string());
    }
    v
}

type Matched<'a> = (
    Platform,
    String,
    HashMap<&'a str, String>,
    Option<ChatMessage>,
);

/// Vérifie le déclencheur ; renvoie plateforme, chaîne, variables de gabarit et message éventuel.
fn match_trigger<'a>(trigger: &Trigger, event: &AutoEvent) -> Option<Matched<'a>> {
    match (trigger, event) {
        (Trigger::Message { contains }, AutoEvent::Message(m)) => {
            if !contains.is_empty() && !m.text.to_lowercase().contains(&contains.to_lowercase()) {
                return None;
            }
            let mut vars = message_vars(m, "");
            vars.remove("args");
            Some((
                m.platform.clone(),
                m.channel_id.clone(),
                vars,
                Some(m.clone()),
            ))
        }
        (Trigger::Follow, AutoEvent::Stream(e)) if matches!(e.kind, StreamEventKind::Follow) => {
            Some(stream_match(e))
        }
        (Trigger::Sub, AutoEvent::Stream(e))
            if matches!(
                e.kind,
                StreamEventKind::Sub { .. } | StreamEventKind::Gift { .. }
            ) =>
        {
            let (p, c, mut vars, m) = stream_match(e);
            if let StreamEventKind::Sub { months } = e.kind {
                vars.insert("months", months.to_string());
            }
            Some((p, c, vars, m))
        }
        (Trigger::Raid { min_viewers }, AutoEvent::Stream(e)) => match e.kind {
            StreamEventKind::Raid { viewers } if viewers >= *min_viewers => {
                let (p, c, mut vars, m) = stream_match(e);
                vars.insert("viewers", viewers.to_string());
                Some((p, c, vars, m))
            }
            _ => None,
        },
        (
            Trigger::StreamStart,
            AutoEvent::StreamStarted {
                channel_id,
                platform,
            },
        )
        | (
            Trigger::StreamEnd,
            AutoEvent::StreamEnded {
                channel_id,
                platform,
            },
        ) => {
            let mut vars = HashMap::new();
            vars.insert("platform", platform.to_string());
            Some((platform.clone(), channel_id.clone(), vars, None))
        }
        _ => None,
    }
}

fn stream_match<'a>(e: &StreamEvent) -> Matched<'a> {
    let mut vars = HashMap::new();
    vars.insert("user", e.who.clone());
    vars.insert("platform", e.platform.to_string());
    (e.platform.clone(), e.channel_id.clone(), vars, None)
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_000_000;

    fn chan(platform: Platform, login: &str) -> Channel {
        Channel {
            id: format!("{platform}:{login}"),
            platform,
            external_id: login.into(),
            display_name: login.into(),
        }
    }

    fn msg(platform: Platform, login: &str, who: &str, text: &str) -> ChatMessage {
        ChatMessage {
            id: "m1".into(),
            channel_id: format!("{platform}:{login}"),
            platform,
            author: ChatAuthor {
                external_id: format!("id-{who}"),
                display_name: who.into(),
                is_moderator: false,
                is_subscriber: false,
            },
            text: text.into(),
            timestamp_ms: T0,
            emotes: vec![],
            badges: vec![],
            room_id: None,
        }
    }

    fn cmd(name: &str, response: &str) -> ChatCommand {
        ChatCommand {
            id: name.into(),
            name: name.into(),
            response: response.into(),
            enabled: true,
            cooldown_secs: 0,
            user_cooldown_secs: 0,
            min_role: Role::Everyone,
            platforms: vec![],
            reply: Target::Source,
        }
    }

    fn rule(id: &str, trigger: Trigger, action: Action) -> Rule {
        Rule {
            id: id.into(),
            name: id.into(),
            enabled: true,
            trigger,
            action,
            platforms: vec![],
            cooldown_secs: 0,
        }
    }

    fn say(text: &str) -> Action {
        Action::SendMessage {
            target: Target::All,
            text: text.into(),
        }
    }

    fn engine() -> (Engine, Arc<ManualClock>) {
        let clock = ManualClock::new(T0);
        let mut e = Engine::new(clock.clone());
        e.set_channels(vec![
            chan(Platform::Twitch, "garder"),
            chan(Platform::Kick, "garder"),
        ]);
        (e, clock)
    }

    fn stream_event(platform: Platform, kind: StreamEventKind) -> AutoEvent {
        AutoEvent::Stream(StreamEvent {
            id: "e".into(),
            channel_id: format!("{platform}:garder"),
            platform,
            who: "Bob".into(),
            kind,
            message: None,
            timestamp_ms: T0,
        })
    }

    #[test]
    fn render_replaces_known_and_keeps_unknown() {
        let mut v = HashMap::new();
        v.insert("user", "Bob".to_string());
        assert_eq!(
            render("Salut {user} {inconnu} {", &v),
            "Salut Bob {inconnu} {"
        );
        assert_eq!(render("{user}{user}", &v), "BobBob");
        assert_eq!(render("sans variable", &v), "sans variable");
        assert_eq!(render("é{user}é", &v), "éBobé");
    }

    #[test]
    fn parse_command_cases() {
        assert_eq!(
            parse_command("!Discord"),
            Some(("discord".into(), "".into()))
        );
        assert_eq!(
            parse_command("  !so  Bob  hello "),
            Some(("so".into(), "Bob  hello".into()))
        );
        assert_eq!(parse_command("! rien"), None);
        assert_eq!(parse_command("!"), None);
        assert_eq!(parse_command("salut !discord"), None);
    }

    #[test]
    fn command_replies_on_source_channel_with_variables() {
        let (mut e, _) = engine();
        e.set_commands(vec![cmd(
            "so",
            "Allez voir {arg1} ! (demandé par {user} sur {platform})",
        )]);
        let fx = e.handle(&AutoEvent::Message(msg(
            Platform::Kick,
            "garder",
            "Alice",
            "!SO Bob extra",
        )));
        assert_eq!(
            fx,
            vec![Effect::Send {
                scope: Scope::Channel("kick:garder".into()),
                text: "Allez voir Bob ! (demandé par Alice sur kick)".into(),
                origin: "!so".into()
            }]
        );
    }

    #[test]
    fn command_defined_once_works_on_every_platform() {
        let (mut e, _) = engine();
        e.set_commands(vec![cmd("discord", "discord.gg/x")]);
        for p in [
            Platform::Twitch,
            Platform::Kick,
            Platform::Youtube,
            Platform::Tiktok,
        ] {
            assert_eq!(
                e.handle(&AutoEvent::Message(msg(p, "garder", "A", "!discord")))
                    .len(),
                1
            );
        }
    }

    #[test]
    fn command_reply_can_target_all_or_platforms() {
        let (mut e, _) = engine();
        let mut c = cmd("hello", "hi");
        c.reply = Target::All;
        let mut d = cmd("yt", "hi");
        d.reply = Target::Platforms {
            platforms: vec![Platform::Youtube],
        };
        e.set_commands(vec![c, d]);
        let fx = e.handle(&AutoEvent::Message(msg(
            Platform::Twitch,
            "garder",
            "A",
            "!hello",
        )));
        assert!(matches!(
            &fx[0],
            Effect::Send {
                scope: Scope::AllChannels,
                ..
            }
        ));
        let fx = e.handle(&AutoEvent::Message(msg(
            Platform::Twitch,
            "garder",
            "A",
            "!yt",
        )));
        assert!(
            matches!(&fx[0], Effect::Send { scope: Scope::Platforms(p), .. } if p == &vec![Platform::Youtube])
        );
    }

    #[test]
    fn disabled_unknown_and_platform_restricted_commands_are_ignored() {
        let (mut e, _) = engine();
        let mut off = cmd("off", "x");
        off.enabled = false;
        let mut only_twitch = cmd("tw", "x");
        only_twitch.platforms = vec![Platform::Twitch];
        e.set_commands(vec![off, only_twitch]);
        assert!(e
            .handle(&AutoEvent::Message(msg(
                Platform::Twitch,
                "garder",
                "A",
                "!off"
            )))
            .is_empty());
        assert!(e
            .handle(&AutoEvent::Message(msg(
                Platform::Twitch,
                "garder",
                "A",
                "!nope"
            )))
            .is_empty());
        assert!(e
            .handle(&AutoEvent::Message(msg(
                Platform::Kick,
                "garder",
                "A",
                "!tw"
            )))
            .is_empty());
        assert_eq!(
            e.handle(&AutoEvent::Message(msg(
                Platform::Twitch,
                "garder",
                "A",
                "!tw"
            )))
            .len(),
            1
        );
    }

    #[test]
    fn global_cooldown_blocks_then_releases() {
        let (mut e, clock) = engine();
        let mut c = cmd("lurk", "x");
        c.cooldown_secs = 30;
        e.set_commands(vec![c]);
        let m = |who| AutoEvent::Message(msg(Platform::Twitch, "garder", who, "!lurk"));
        assert_eq!(e.handle(&m("A")).len(), 1);
        assert!(
            e.handle(&m("B")).is_empty(),
            "le cooldown global bloque aussi les autres"
        );
        clock.advance_secs(29);
        assert!(e.handle(&m("B")).is_empty());
        clock.advance_secs(1);
        assert_eq!(e.handle(&m("B")).len(), 1);
    }

    #[test]
    fn global_cooldown_is_shared_across_platforms() {
        let (mut e, _) = engine();
        let mut c = cmd("lurk", "x");
        c.cooldown_secs = 30;
        e.set_commands(vec![c]);
        assert_eq!(
            e.handle(&AutoEvent::Message(msg(
                Platform::Twitch,
                "garder",
                "A",
                "!lurk"
            )))
            .len(),
            1
        );
        assert!(e
            .handle(&AutoEvent::Message(msg(
                Platform::Kick,
                "garder",
                "B",
                "!lurk"
            )))
            .is_empty());
    }

    #[test]
    fn user_cooldown_is_per_user() {
        let (mut e, clock) = engine();
        let mut c = cmd("roll", "x");
        c.user_cooldown_secs = 10;
        e.set_commands(vec![c]);
        let m = |who| AutoEvent::Message(msg(Platform::Twitch, "garder", who, "!roll"));
        assert_eq!(e.handle(&m("A")).len(), 1);
        assert!(e.handle(&m("A")).is_empty());
        assert_eq!(
            e.handle(&m("B")).len(),
            1,
            "un autre utilisateur n'est pas bloqué"
        );
        clock.advance_secs(10);
        assert_eq!(e.handle(&m("A")).len(), 1);
    }

    #[test]
    fn cooldown_not_consumed_by_denied_attempts() {
        let (mut e, _) = engine();
        let mut c = cmd("mod", "x");
        c.min_role = Role::Moderator;
        c.cooldown_secs = 60;
        e.set_commands(vec![c]);
        assert!(e
            .handle(&AutoEvent::Message(msg(
                Platform::Twitch,
                "garder",
                "viewer",
                "!mod"
            )))
            .is_empty());
        let mut m = msg(Platform::Twitch, "garder", "modo", "!mod");
        m.author.is_moderator = true;
        assert_eq!(
            e.handle(&AutoEvent::Message(m)).len(),
            1,
            "le refus n'a pas déclenché le cooldown"
        );
    }

    #[test]
    fn role_permissions() {
        let (mut e, _) = engine();
        let cmds = [
            ("sub", Role::Subscriber),
            ("mod", Role::Moderator),
            ("bc", Role::Broadcaster),
        ]
        .into_iter()
        .map(|(n, r)| ChatCommand {
            min_role: r,
            ..cmd(n, "x")
        })
        .collect();
        e.set_commands(cmds);
        let send = |e: &mut Engine, who: &str, sub: bool, modo: bool, text: &str| {
            let mut m = msg(Platform::Twitch, "garder", who, text);
            m.author.is_subscriber = sub;
            m.author.is_moderator = modo;
            e.handle(&AutoEvent::Message(m)).len()
        };
        assert_eq!(send(&mut e, "v", false, false, "!sub"), 0);
        assert_eq!(send(&mut e, "s", true, false, "!sub"), 1);
        assert_eq!(send(&mut e, "s", true, false, "!mod"), 0);
        assert_eq!(
            send(&mut e, "m", false, true, "!sub"),
            1,
            "un modérateur passe les commandes abonné"
        );
        assert_eq!(send(&mut e, "m", false, true, "!mod"), 1);
        assert_eq!(send(&mut e, "m", false, true, "!bc"), 0);
        assert_eq!(
            send(&mut e, "Garder", false, false, "!bc"),
            1,
            "le diffuseur est reconnu à son pseudo"
        );
        assert_eq!(send(&mut e, "garder", false, false, "!mod"), 1);
    }

    #[test]
    fn broadcaster_recognised_by_platform_id() {
        let a = ChatAuthor {
            external_id: "garder".into(),
            display_name: "Autre".into(),
            is_moderator: false,
            is_subscriber: false,
        };
        assert_eq!(
            role_of(&a, Some(&chan(Platform::Kick, "garder"))),
            Role::Broadcaster
        );
        assert_eq!(role_of(&a, None), Role::Everyone);
        assert!(
            Role::Broadcaster > Role::Moderator
                && Role::Moderator > Role::Subscriber
                && Role::Subscriber > Role::Everyone
        );
    }

    #[test]
    fn message_rule_contains_is_case_insensitive_and_empty_matches_all() {
        let (mut e, _) = engine();
        e.set_rules(vec![
            rule(
                "a",
                Trigger::Message {
                    contains: "Bonjour".into(),
                },
                say("coucou {user}"),
            ),
            rule(
                "b",
                Trigger::Message {
                    contains: "".into(),
                },
                say("tout"),
            ),
        ]);
        let fx = e.handle(&AutoEvent::Message(msg(
            Platform::Twitch,
            "garder",
            "Al",
            "oh BONJOUR à tous",
        )));
        assert_eq!(fx.len(), 2);
        assert!(
            matches!(&fx[0], Effect::Send { text, scope: Scope::AllChannels, .. } if text == "coucou Al")
        );
        let fx = e.handle(&AutoEvent::Message(msg(
            Platform::Twitch,
            "garder",
            "Al",
            "salut",
        )));
        assert_eq!(fx.len(), 1);
    }

    #[test]
    fn rule_platform_filter_and_disabled() {
        let (mut e, _) = engine();
        let mut r = rule("a", Trigger::Follow, say("merci"));
        r.platforms = vec![Platform::Kick];
        let mut off = rule("b", Trigger::Follow, say("off"));
        off.enabled = false;
        e.set_rules(vec![r, off]);
        assert!(e
            .handle(&stream_event(Platform::Twitch, StreamEventKind::Follow))
            .is_empty());
        assert_eq!(
            e.handle(&stream_event(Platform::Kick, StreamEventKind::Follow))
                .len(),
            1
        );
    }

    #[test]
    fn follow_sub_and_raid_triggers() {
        let (mut e, _) = engine();
        e.set_rules(vec![
            rule(
                "f",
                Trigger::Follow,
                Action::SendMessage {
                    target: Target::Source,
                    text: "bienvenue {user}".into(),
                },
            ),
            rule("s", Trigger::Sub, say("sub {user} {months}")),
            rule(
                "r",
                Trigger::Raid { min_viewers: 10 },
                say("raid {viewers} de {user}"),
            ),
        ]);
        let fx = e.handle(&stream_event(Platform::Twitch, StreamEventKind::Follow));
        assert_eq!(
            fx,
            vec![Effect::Send {
                scope: Scope::Channel("twitch:garder".into()),
                text: "bienvenue Bob".into(),
                origin: "f".into()
            }]
        );
        let fx = e.handle(&stream_event(
            Platform::Twitch,
            StreamEventKind::Sub { months: 3 },
        ));
        assert!(matches!(&fx[0], Effect::Send { text, .. } if text == "sub Bob 3"));
        let fx = e.handle(&stream_event(
            Platform::Twitch,
            StreamEventKind::Gift { recipient: None },
        ));
        assert_eq!(
            fx.len(),
            1,
            "un don de subs déclenche aussi le déclencheur sub"
        );
        assert!(e
            .handle(&stream_event(
                Platform::Twitch,
                StreamEventKind::Raid { viewers: 9 }
            ))
            .is_empty());
        let fx = e.handle(&stream_event(
            Platform::Twitch,
            StreamEventKind::Raid { viewers: 10 },
        ));
        assert!(matches!(&fx[0], Effect::Send { text, .. } if text == "raid 10 de Bob"));
    }

    #[test]
    fn rule_cooldown() {
        let (mut e, clock) = engine();
        let mut r = rule("f", Trigger::Follow, say("x"));
        r.cooldown_secs = 5;
        e.set_rules(vec![r]);
        let ev = stream_event(Platform::Twitch, StreamEventKind::Follow);
        assert_eq!(e.handle(&ev).len(), 1);
        assert!(e.handle(&ev).is_empty());
        clock.advance_secs(5);
        assert_eq!(e.handle(&ev).len(), 1);
    }

    #[test]
    fn moderation_rules_target_the_author() {
        let (mut e, _) = engine();
        e.set_rules(vec![
            rule(
                "t",
                Trigger::Message {
                    contains: "spam".into(),
                },
                Action::Timeout { seconds: 600 },
            ),
            rule(
                "d",
                Trigger::Message {
                    contains: "spam".into(),
                },
                Action::DeleteMessage,
            ),
            rule(
                "b",
                Trigger::Message {
                    contains: "scam".into(),
                },
                Action::Ban {
                    reason: Some("arnaque".into()),
                },
            ),
        ]);
        let fx = e.handle(&AutoEvent::Message(msg(
            Platform::Kick,
            "garder",
            "Troll",
            "SPAM spam",
        )));
        assert_eq!(fx.len(), 2);
        assert_eq!(
            fx[0],
            Effect::Moderate {
                channel_id: "kick:garder".into(),
                kind: ModerationKind::Timeout {
                    user_id: "id-Troll".into(),
                    seconds: 600
                },
                origin: "t".into()
            }
        );
        assert!(
            matches!(&fx[1], Effect::Moderate { kind: ModerationKind::DeleteMessage { message_id }, .. } if message_id == "m1")
        );
        let fx = e.handle(&AutoEvent::Message(msg(
            Platform::Kick,
            "garder",
            "Troll",
            "scam",
        )));
        assert!(
            matches!(&fx[0], Effect::Moderate { kind: ModerationKind::Ban { reason: Some(r), .. }, .. } if r == "arnaque")
        );
    }

    #[test]
    fn moderation_rules_spare_moderators_and_broadcaster() {
        let (mut e, _) = engine();
        e.set_rules(vec![rule(
            "t",
            Trigger::Message {
                contains: "http".into(),
            },
            Action::Timeout { seconds: 60 },
        )]);
        let mut m = msg(Platform::Twitch, "garder", "Modo", "http://x");
        m.author.is_moderator = true;
        assert!(e.handle(&AutoEvent::Message(m)).is_empty());
        assert!(e
            .handle(&AutoEvent::Message(msg(
                Platform::Twitch,
                "garder",
                "Garder",
                "http://x"
            )))
            .is_empty());
        assert!(e.is_broadcaster(&msg(Platform::Twitch, "garder", "garder", "x")));
        assert_eq!(
            e.handle(&AutoEvent::Message(msg(
                Platform::Twitch,
                "garder",
                "Viewer",
                "http://x"
            )))
            .len(),
            1
        );
    }

    #[test]
    fn timer_platform_filter_restricts_scope() {
        let (mut e, clock) = engine();
        let mut r = rule("t", Trigger::Timer { every_secs: 1 }, say("x"));
        r.platforms = vec![Platform::Kick];
        e.set_rules(vec![r]);
        clock.advance_secs(1);
        assert_eq!(
            e.tick(),
            vec![Effect::Send {
                scope: Scope::Platforms(vec![Platform::Kick]),
                text: "x".into(),
                origin: "t".into()
            }]
        );
    }

    #[test]
    fn command_and_rule_can_both_fire_on_the_same_message() {
        let (mut e, _) = engine();
        e.set_commands(vec![cmd("hi", "hello")]);
        e.set_rules(vec![rule(
            "a",
            Trigger::Message {
                contains: "!hi".into(),
            },
            say("log"),
        )]);
        assert_eq!(
            e.handle(&AutoEvent::Message(msg(
                Platform::Twitch,
                "garder",
                "A",
                "!hi"
            )))
            .len(),
            2
        );
    }

    #[test]
    fn timer_fires_after_interval_without_burst() {
        let (mut e, clock) = engine();
        e.set_rules(vec![rule(
            "t",
            Trigger::Timer { every_secs: 60 },
            say("pense au follow"),
        )]);
        assert!(e.tick().is_empty(), "pas de déclenchement immédiat");
        clock.advance_secs(59);
        assert!(e.tick().is_empty());
        clock.advance_secs(1);
        let fx = e.tick();
        assert_eq!(
            fx,
            vec![Effect::Send {
                scope: Scope::AllChannels,
                text: "pense au follow".into(),
                origin: "t".into()
            }]
        );
        assert!(e.tick().is_empty(), "une seule fois par échéance");
        clock.advance_secs(600);
        assert_eq!(e.tick().len(), 1, "pas de rattrapage en rafale");
        assert!(e.tick().is_empty());
        clock.advance_secs(60);
        assert_eq!(e.tick().len(), 1);
    }

    #[test]
    fn timer_with_source_target_broadcasts_and_disabled_timer_is_silent() {
        let (mut e, clock) = engine();
        let mut off = rule("off", Trigger::Timer { every_secs: 1 }, say("x"));
        off.enabled = false;
        e.set_rules(vec![
            rule(
                "t",
                Trigger::Timer { every_secs: 1 },
                Action::SendMessage {
                    target: Target::Source,
                    text: "tic".into(),
                },
            ),
            off,
        ]);
        clock.advance_secs(1);
        let fx = e.tick();
        assert_eq!(fx.len(), 1);
        assert!(matches!(
            &fx[0],
            Effect::Send {
                scope: Scope::AllChannels,
                ..
            }
        ));
    }

    #[test]
    fn set_rules_preserves_unchanged_timers_and_resets_changed_ones() {
        let (mut e, clock) = engine();
        e.set_rules(vec![rule("t", Trigger::Timer { every_secs: 60 }, say("x"))]);
        clock.advance_secs(50);
        e.set_rules(vec![rule("t", Trigger::Timer { every_secs: 60 }, say("y"))]);
        clock.advance_secs(10);
        assert_eq!(e.tick().len(), 1, "l'échéance d'origine est conservée");
        clock.advance_secs(30);
        e.set_rules(vec![rule(
            "t",
            Trigger::Timer { every_secs: 120 },
            say("y"),
        )]);
        clock.advance_secs(60);
        assert!(
            e.tick().is_empty(),
            "intervalle modifié : le minuteur repart de zéro"
        );
        clock.advance_secs(60);
        assert_eq!(e.tick().len(), 1);
        e.set_rules(vec![]);
        clock.advance_secs(1000);
        assert!(e.tick().is_empty());
    }

    #[test]
    fn stream_start_and_end_on_transitions_only() {
        let (mut e, _) = engine();
        e.set_rules(vec![
            rule("on", Trigger::StreamStart, say("on {platform}")),
            rule("off", Trigger::StreamEnd, say("off")),
        ]);
        let p = Platform::Twitch;
        assert!(
            e.observe_live("twitch:garder", &p, true).is_empty(),
            "déjà en live au lancement de l'app"
        );
        assert!(e.observe_live("twitch:garder", &p, true).is_empty());
        let fx = e.observe_live("twitch:garder", &p, false);
        assert!(matches!(&fx[0], Effect::Send { text, .. } if text == "off"));
        assert!(e.observe_live("twitch:garder", &p, false).is_empty());
        let fx = e.observe_live("twitch:garder", &p, true);
        assert!(matches!(&fx[0], Effect::Send { text, .. } if text == "on twitch"));
        // Une autre chaîne a son propre état.
        assert!(e
            .observe_live("kick:garder", &Platform::Kick, false)
            .is_empty());
        assert_eq!(
            e.observe_live("kick:garder", &Platform::Kick, true).len(),
            1
        );
    }

    #[test]
    fn rules_do_not_cross_trigger_kinds() {
        let (mut e, _) = engine();
        e.set_rules(vec![
            rule("f", Trigger::Follow, say("x")),
            rule("on", Trigger::StreamStart, say("y")),
        ]);
        assert!(e
            .handle(&AutoEvent::Message(msg(
                Platform::Twitch,
                "garder",
                "A",
                "follow"
            )))
            .is_empty());
        assert!(e
            .handle(&stream_event(
                Platform::Twitch,
                StreamEventKind::Raid { viewers: 50 }
            ))
            .is_empty());
    }

    #[test]
    fn validation() {
        assert!(rule("a", Trigger::Follow, say("x")).validate().is_ok());
        assert!(rule("a", Trigger::Follow, say("  ")).validate().is_err());
        assert!(rule("a", Trigger::Timer { every_secs: 0 }, say("x"))
            .validate()
            .is_err());
        assert!(rule("a", Trigger::Follow, Action::Timeout { seconds: 5 })
            .validate()
            .is_err());
        assert!(rule(
            "a",
            Trigger::Message {
                contains: "".into()
            },
            Action::DeleteMessage
        )
        .validate()
        .is_ok());
        let mut r = rule("a", Trigger::Follow, say("x"));
        r.name = " ".into();
        assert!(r.validate().is_err());

        assert!(cmd("ok_1", "x").validate().is_ok());
        assert!(cmd("a b", "x").validate().is_err());
        assert!(cmd("", "x").validate().is_err());
        assert!(cmd("!x", "x").validate().is_err());
        assert!(cmd("x", " ").validate().is_err());
        assert_eq!(ChatCommand::normalize_name(" !Discord "), "discord");
    }

    #[test]
    fn serde_roundtrip_of_rule() {
        let r = rule(
            "a",
            Trigger::Raid { min_viewers: 5 },
            Action::SendMessage {
                target: Target::Platforms {
                    platforms: vec![Platform::Kick],
                },
                text: "x".into(),
            },
        );
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<Rule>(&json).unwrap(), r);
        let minimal: Rule = serde_json::from_str(
            r#"{"id":"1","name":"n","enabled":true,"trigger":{"type":"follow"},"action":{"type":"send_message","text":"hi"}}"#,
        )
        .unwrap();
        assert_eq!(
            minimal.action,
            Action::SendMessage {
                target: Target::Source,
                text: "hi".into()
            }
        );
    }
}
