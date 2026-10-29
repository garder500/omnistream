//! Automod unifié : un moteur de règles évalué sur chaque message, quelle que
//! soit la plateforme. Chaque règle porte un score et une action
//! (signaler, supprimer, timeout) traduite en [`ModerationKind`].

use std::collections::{HashMap, VecDeque};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::domain::{ChatMessage, ModerationKind, Platform};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleKind {
    /// Expression régulière, insensible à la casse.
    Regex { pattern: String },
    /// Mots ou expressions bannis (insensible à la casse et aux accents).
    BannedWords { words: Vec<String> },
    /// Tout lien dont le domaine n'est pas dans la liste blanche.
    Links { allowed_domains: Vec<String> },
    /// Trop de majuscules.
    Caps { min_letters: u32, ratio_percent: u32 },
    /// Mot répété ou caractère étiré.
    Repeat { min_words: u32, char_run: u32 },
    /// Même auteur (normalisé), même texte, sur plusieurs chaînes dans une fenêtre.
    CrossPlatformSpam { window_secs: u32, min_channels: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleAction {
    Flag,
    Delete,
    Timeout { seconds: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Points ajoutés au score du message quand la règle se déclenche.
    pub score: u32,
    pub kind: RuleKind,
    pub action: RuleAction,
    /// Plateformes concernées ; vide = toutes.
    #[serde(default)]
    pub platforms: Vec<Platform>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleMatch {
    pub rule_id: String,
    pub name: String,
    pub score: u32,
    pub action: RuleAction,
}

/// Résultat de l'évaluation d'un message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub matches: Vec<RuleMatch>,
    pub score: u32,
}

impl Verdict {
    pub fn is_clean(&self) -> bool {
        self.matches.is_empty()
    }

    /// Noms des règles déclenchées, pour l'étiquette de la file de modération.
    pub fn reason(&self) -> Option<String> {
        if self.matches.is_empty() {
            return None;
        }
        Some(self.matches.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(" + "))
    }

    pub fn delete(&self) -> bool {
        self.matches.iter().any(|m| m.action == RuleAction::Delete)
    }

    /// Durée de timeout la plus longue demandée par les règles déclenchées.
    pub fn timeout_secs(&self) -> Option<u32> {
        self.matches
            .iter()
            .filter_map(|m| match m.action {
                RuleAction::Timeout { seconds } => Some(seconds),
                _ => None,
            })
            .max()
    }

    /// Actions de modération à appliquer sur la chaîne du message.
    pub fn moderation_kinds(&self, msg: &ChatMessage) -> Vec<ModerationKind> {
        let mut kinds = Vec::new();
        if let Some(seconds) = self.timeout_secs() {
            kinds.push(ModerationKind::Timeout { user_id: msg.author.external_id.clone(), seconds });
        }
        if self.delete() {
            kinds.push(ModerationKind::DeleteMessage { message_id: msg.id.clone() });
        }
        kinds
    }
}

/// Auteur normalisé pour reconnaître la même personne d'une plateforme à l'autre.
pub fn normalize_author(name: &str) -> String {
    fold(name).chars().filter(|c| c.is_alphanumeric()).collect()
}

fn fold_char(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

/// Minuscules et sans accents courants.
fn fold(s: &str) -> String {
    s.to_lowercase().chars().map(fold_char).collect()
}

/// Texte réduit à des mots séparés par un espace, entouré d'espaces.
fn word_text(s: &str) -> String {
    let words: Vec<String> = fold(s)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    format!(" {} ", words.join(" "))
}

fn link_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:https?://|www\.)[^\s]+|\b(?:[a-z0-9-]+\.)+(?:com|net|org|fr|tv|gg|io|ly|me|co|xyz|link|ru|tk|cc|to|be|ink|info|shop|live)\b(?:/[^\s]*)?",
        )
        .expect("regex de liens valide")
    })
}

/// Domaines (minuscules, sans `www.`) des liens présents dans le texte.
pub fn extract_domains(text: &str) -> Vec<String> {
    link_regex()
        .find_iter(text)
        .map(|m| {
            let s = m.as_str().to_lowercase();
            let s = s.trim_start_matches("https://").trim_start_matches("http://");
            let authority = s.split(['/', '?', '#']).next().unwrap_or("");
            let host = authority.rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("");
            host.trim_start_matches("www.").trim_end_matches('.').to_string()
        })
        .filter(|h| !h.is_empty())
        .collect()
}

fn domain_allowed(domain: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|a| {
        let a = a.trim().to_lowercase();
        let a = a.trim_start_matches("www.");
        !a.is_empty() && (domain == a || domain.ends_with(&format!(".{a}")))
    })
}

/// Vérifie qu'une règle est utilisable (regex valide, nom non vide...).
pub fn validate_rule(rule: &Rule) -> Result<()> {
    if rule.id.trim().is_empty() {
        return Err(Error::Other("règle sans identifiant".into()));
    }
    if rule.name.trim().is_empty() {
        return Err(Error::Other("le nom de la règle est obligatoire".into()));
    }
    match &rule.kind {
        RuleKind::Regex { pattern } => {
            if compile_regex(pattern)?.is_match("") {
                return Err(Error::Other("l'expression régulière correspond au texte vide".into()));
            }
        }
        RuleKind::BannedWords { words } if words.iter().all(|w| word_text(w).trim().is_empty()) => {
            return Err(Error::Other("ajoutez au moins un mot banni".into()));
        }
        RuleKind::Caps { ratio_percent, min_letters } if *ratio_percent == 0 || *ratio_percent > 100 || *min_letters == 0 => {
            return Err(Error::Other("majuscules : ratio entre 1 et 100 % et au moins 1 lettre".into()));
        }
        RuleKind::Repeat { min_words: 0, char_run: 0 } => {
            return Err(Error::Other("répétitions : renseignez au moins un seuil".into()));
        }
        RuleKind::CrossPlatformSpam { min_channels, window_secs } if *min_channels < 2 || *window_secs == 0 => {
            return Err(Error::Other("le spam inter-plateformes demande au moins 2 chaînes et une fenêtre > 0".into()));
        }
        _ => {}
    }
    if let RuleAction::Timeout { seconds: 0 } = rule.action {
        return Err(Error::Other("la durée du timeout doit être supérieure à 0".into()));
    }
    Ok(())
}

fn compile_regex(pattern: &str) -> Result<Regex> {
    if pattern.is_empty() {
        return Err(Error::Other("l'expression régulière est vide".into()));
    }
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| Error::Other(format!("expression régulière invalide: {e}")))
}

struct Compiled {
    rule: Rule,
    regex: Option<Regex>,
    /// Mots bannis normalisés, entourés d'espaces.
    words: Vec<String>,
}

struct Seen {
    at_ms: i64,
    channel_id: String,
    text: String,
}

/// Nombre maximal de messages mémorisés par auteur.
const HISTORY_MAX: usize = 50;

#[derive(Default)]
pub struct AutoMod {
    rules: Vec<Compiled>,
    history: HashMap<String, VecDeque<Seen>>,
    /// Plus longue fenêtre des règles de spam inter-plateformes actives (0 = aucune).
    max_window_ms: i64,
}

impl AutoMod {
    pub fn new(rules: Vec<Rule>) -> Self {
        let mut a = Self::default();
        a.set_rules(rules);
        a
    }

    /// Remplace les règles ; les règles invalides sont ignorées.
    pub fn set_rules(&mut self, rules: Vec<Rule>) {
        self.history.clear();
        self.rules = rules
            .into_iter()
            .filter(|r| validate_rule(r).is_ok())
            .map(|rule| {
                let regex = match &rule.kind {
                    RuleKind::Regex { pattern } => compile_regex(pattern).ok(),
                    _ => None,
                };
                let words = match &rule.kind {
                    RuleKind::BannedWords { words } => {
                        words.iter().map(|w| word_text(w)).filter(|w| !w.trim().is_empty()).collect()
                    }
                    _ => Vec::new(),
                };
                Compiled { rule, regex, words }
            })
            .collect();
        self.max_window_ms = self
            .rules
            .iter()
            .filter(|c| c.rule.enabled)
            .filter_map(|c| match c.rule.kind {
                RuleKind::CrossPlatformSpam { window_secs, .. } => Some(i64::from(window_secs) * 1000),
                _ => None,
            })
            .max()
            .unwrap_or(0);
    }

    /// Évalue un message et mémorise son passage (pour le spam inter-plateformes).
    /// Les modérateurs ne sont jamais sanctionnés.
    pub fn evaluate(&mut self, msg: &ChatMessage) -> Verdict {
        if msg.author.is_moderator {
            return Verdict::default();
        }
        let author = normalize_author(&msg.author.display_name);
        let text_key = word_text(&msg.text);
        let verdict = self.run(msg, Some((&author, &text_key)));
        if !author.is_empty() && self.max_window_ms > 0 {
            self.prune(msg.timestamp_ms);
            let hist = self.history.entry(author).or_default();
            hist.push_back(Seen { at_ms: msg.timestamp_ms, channel_id: msg.channel_id.clone(), text: text_key });
            while hist.len() > HISTORY_MAX {
                hist.pop_front();
            }
        }
        verdict
    }

    /// Oublie les messages plus vieux que la plus longue fenêtre configurée.
    fn prune(&mut self, now_ms: i64) {
        let limit = self.max_window_ms;
        for hist in self.history.values_mut() {
            while hist.front().is_some_and(|s| now_ms - s.at_ms > limit) {
                hist.pop_front();
            }
        }
        if self.history.len() > 1024 {
            self.history.retain(|_, h| !h.is_empty());
        }
    }

    /// Comme [`AutoMod::evaluate`] sans toucher à l'historique ni exempter les modérateurs
    /// (aperçu dans l'éditeur). Le spam inter-plateformes n'y est pas évalué.
    pub fn preview(&self, msg: &ChatMessage) -> Verdict {
        self.run(msg, None)
    }

    fn run(&self, msg: &ChatMessage, history: Option<(&str, &str)>) -> Verdict {
        let mut verdict = Verdict::default();
        for c in &self.rules {
            let r = &c.rule;
            if !r.enabled || (!r.platforms.is_empty() && !r.platforms.contains(&msg.platform)) {
                continue;
            }
            let hit = match &r.kind {
                RuleKind::Regex { .. } => c.regex.as_ref().is_some_and(|re| re.is_match(&msg.text)),
                RuleKind::BannedWords { .. } => {
                    let t = word_text(&msg.text);
                    c.words.iter().any(|w| t.contains(w.as_str()))
                }
                RuleKind::Links { allowed_domains } => {
                    extract_domains(&msg.text).iter().any(|d| !domain_allowed(d, allowed_domains))
                }
                RuleKind::Caps { min_letters, ratio_percent } => {
                    let letters: Vec<char> = msg.text.chars().filter(|c| c.is_alphabetic()).collect();
                    let upper = letters.iter().filter(|c| c.is_uppercase()).count();
                    letters.len() >= *min_letters as usize && upper * 100 >= letters.len() * *ratio_percent as usize
                }
                RuleKind::Repeat { min_words, char_run } => repeats(&msg.text, *min_words, *char_run),
                RuleKind::CrossPlatformSpam { window_secs, min_channels } => match history {
                    Some((author, text_key)) => self.cross_spam(msg, author, text_key, *window_secs, *min_channels),
                    None => false,
                },
            };
            if hit {
                verdict.score += r.score;
                verdict.matches.push(RuleMatch {
                    rule_id: r.id.clone(),
                    name: r.name.clone(),
                    score: r.score,
                    action: r.action,
                });
            }
        }
        verdict
    }

    fn cross_spam(&self, msg: &ChatMessage, author: &str, text_key: &str, window_secs: u32, min_channels: u32) -> bool {
        if author.is_empty() || text_key.trim().is_empty() {
            return false;
        }
        let Some(hist) = self.history.get(author) else { return false };
        let window_ms = i64::from(window_secs) * 1000;
        let mut channels: Vec<&str> = vec![msg.channel_id.as_str()];
        for s in hist {
            if s.text == text_key
                && (msg.timestamp_ms - s.at_ms).abs() <= window_ms
                && !channels.contains(&s.channel_id.as_str())
            {
                channels.push(&s.channel_id);
            }
        }
        channels.len() >= min_channels as usize
    }
}

fn repeats(text: &str, min_words: u32, char_run: u32) -> bool {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    if min_words > 0 && words.len() >= min_words as usize && words.iter().all(|w| *w == words[0]) {
        return true;
    }
    if char_run == 0 {
        return false;
    }
    let mut run = 1;
    let mut prev = None;
    for c in text.chars() {
        run = if Some(c) == prev { run + 1 } else { 1 };
        if run >= char_run {
            return true;
        }
        prev = Some(c);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ChatAuthor;

    fn msg(platform: Platform, channel: &str, author: &str, text: &str, ts: i64) -> ChatMessage {
        ChatMessage {
            id: format!("m-{ts}"),
            channel_id: channel.into(),
            platform,
            author: ChatAuthor {
                external_id: format!("id-{author}"),
                display_name: author.into(),
                is_moderator: false,
                is_subscriber: false,
            },
            text: text.into(),
            timestamp_ms: ts,
        }
    }

    fn rule(id: &str, kind: RuleKind, action: RuleAction) -> Rule {
        Rule { id: id.into(), name: id.into(), enabled: true, score: 10, kind, action, platforms: vec![] }
    }

    fn one(kind: RuleKind) -> AutoMod {
        AutoMod::new(vec![rule("r", kind, RuleAction::Flag)])
    }

    fn hits(a: &mut AutoMod, text: &str) -> bool {
        !a.evaluate(&msg(Platform::Twitch, "c", "bob", text, 0)).is_clean()
    }

    #[test]
    fn regex_rule_is_case_insensitive_and_rejects_invalid() {
        let mut a = one(RuleKind::Regex { pattern: r"gagn(e|ez) \d+ ?(€|euros)".into() });
        assert!(hits(&mut a, "GAGNEZ 500 euros maintenant"));
        assert!(!hits(&mut a, "gagné une partie"));
        let bad = rule("x", RuleKind::Regex { pattern: "(".into() }, RuleAction::Flag);
        assert!(validate_rule(&bad).is_err());
        assert!(validate_rule(&rule("x", RuleKind::Regex { pattern: "".into() }, RuleAction::Flag)).is_err());
        // une règle invalide est ignorée, sans paniquer
        let a = AutoMod::new(vec![bad]);
        assert!(a.preview(&msg(Platform::Kick, "c", "a", "(", 0)).is_clean());
    }

    #[test]
    fn banned_words_ignore_case_accents_and_substrings() {
        let mut a = one(RuleKind::BannedWords { words: vec!["Connard".into(), "fils de pute".into()] });
        assert!(hits(&mut a, "quel CONNARD"));
        assert!(hits(&mut a, "t'es un fils  de   pute!"));
        assert!(hits(&mut a, "Connard."));
        assert!(!hits(&mut a, "le connardisme est un mot inventé"));
        let mut a = one(RuleKind::BannedWords { words: vec!["éléphant".into()] });
        assert!(hits(&mut a, "un ELEPHANT"));
        assert!(validate_rule(&rule("x", RuleKind::BannedWords { words: vec![" ".into()] }, RuleAction::Flag)).is_err());
    }

    #[test]
    fn links_respect_allowlist() {
        let mut a = one(RuleKind::Links { allowed_domains: vec!["twitch.tv".into(), "WWW.youtube.com".into()] });
        assert!(!hits(&mut a, "regardez https://www.twitch.tv/garder"));
        assert!(!hits(&mut a, "clip sur clips.twitch.tv/abc"));
        assert!(!hits(&mut a, "https://youtube.com/watch?v=1"));
        assert!(hits(&mut a, "achetez des abonnés sur bit.ly/xxxx"));
        assert!(hits(&mut a, "http://eviltwitch.tv/login"));
        assert!(hits(&mut a, "go www.arnaque.fr"));
        assert!(hits(&mut a, "https://twitch.tv.evil.ru/x"));
        assert!(!hits(&mut a, "GG pour le boss !!"));
        assert!(!hits(&mut a, "j'ai fini à 3.5 sur 10"));
    }

    #[test]
    fn extracts_domains() {
        assert_eq!(
            extract_domains("va sur HTTPS://WWW.Exemple.com:8080/a?b#c et bit.ly/zz."),
            vec!["exemple.com", "bit.ly"]
        );
        assert!(extract_domains("rien ici").is_empty());
    }

    #[test]
    fn caps_and_repeat_reproduce_old_heuristic() {
        let mut caps = one(RuleKind::Caps { min_letters: 12, ratio_percent: 80 });
        assert!(hits(&mut caps, "REGARDEZ MA CHAINE REGARDEZ MA CHAINE"));
        assert!(!hits(&mut caps, "LOL"));
        assert!(!hits(&mut caps, "GG pour le boss !!"));
        let mut rep = one(RuleKind::Repeat { min_words: 4, char_run: 15 });
        assert!(hits(&mut rep, "spam spam spam spam"));
        assert!(hits(&mut rep, "aaaaaaaaaaaaaaaaaaaa"));
        assert!(!hits(&mut rep, "spam spam spam"));
        assert!(!hits(&mut rep, "GG pour le boss !!"));
        assert!(validate_rule(&rule("x", RuleKind::Caps { min_letters: 5, ratio_percent: 0 }, RuleAction::Flag)).is_err());
    }

    #[test]
    fn cross_platform_spam_uses_normalized_author() {
        let kind = RuleKind::CrossPlatformSpam { window_secs: 60, min_channels: 2 };
        let mut a = one(kind.clone());
        assert!(a.evaluate(&msg(Platform::Twitch, "twitch:x", "Bob_42", "Suivez-moi sur ma chaîne !", 1_000)).is_clean());
        // même texte (casse/ponctuation différentes), pseudo normalisé identique, autre plateforme
        let second = a.evaluate(&msg(Platform::Kick, "kick:x", "bob42", "suivez moi sur ma chaine", 20_000));
        assert_eq!(second.matches.len(), 1);
        // autre auteur : rien
        assert!(a.evaluate(&msg(Platform::Kick, "kick:x", "alice", "suivez moi sur ma chaine", 21_000)).is_clean());
        // hors fenêtre : rien
        assert!(a.evaluate(&msg(Platform::Youtube, "yt:x", "bob42", "suivez moi sur ma chaine", 200_000)).is_clean());
        // même chaîne répétée : pas du spam inter-chaînes
        let mut b = one(kind);
        b.evaluate(&msg(Platform::Kick, "k", "z", "salut", 0));
        assert!(b.evaluate(&msg(Platform::Kick, "k", "z", "salut", 1000)).is_clean());
        let bad = rule("x", RuleKind::CrossPlatformSpam { window_secs: 60, min_channels: 1 }, RuleAction::Flag);
        assert!(validate_rule(&bad).is_err());
    }

    #[test]
    fn review_regressions() {
        // userinfo ne doit pas permettre de passer la liste blanche
        assert_eq!(extract_domains("https://twitch.tv:443@evil.ru/x"), vec!["evil.ru"]);
        // règles dégénérées refusées
        let bad = [
            RuleKind::Caps { min_letters: 0, ratio_percent: 80 },
            RuleKind::Repeat { min_words: 0, char_run: 0 },
            RuleKind::Regex { pattern: "a*".into() },
        ];
        for k in bad {
            assert!(validate_rule(&rule("x", k, RuleAction::Delete)).is_err());
        }
        // l'historique est élagué hors fenêtre
        let mut a = one(RuleKind::CrossPlatformSpam { window_secs: 10, min_channels: 2 });
        a.evaluate(&msg(Platform::Twitch, "t", "bob", "yo", 0));
        a.evaluate(&msg(Platform::Twitch, "t", "bob", "yo", 100_000));
        assert_eq!(a.history["bob"].len(), 1);
        // sans règle de spam, aucun historique
        let mut b = one(RuleKind::BannedWords { words: vec!["x".into()] });
        b.evaluate(&msg(Platform::Twitch, "t", "bob", "yo", 0));
        assert!(b.history.is_empty());
    }

    #[test]
    fn preview_does_not_record_history() {
        let mut a = one(RuleKind::CrossPlatformSpam { window_secs: 60, min_channels: 2 });
        a.preview(&msg(Platform::Twitch, "t", "bob", "yo", 0));
        assert!(a.evaluate(&msg(Platform::Kick, "k", "bob", "yo", 10)).is_clean());
    }

    #[test]
    fn platform_filter_disabled_rules_and_moderators() {
        let mut r = rule("r", RuleKind::BannedWords { words: vec!["nul".into()] }, RuleAction::Flag);
        r.platforms = vec![Platform::Kick];
        let mut a = AutoMod::new(vec![r.clone()]);
        assert!(a.evaluate(&msg(Platform::Twitch, "c", "a", "nul", 0)).is_clean());
        assert!(!a.evaluate(&msg(Platform::Kick, "c", "a", "nul", 0)).is_clean());
        let mut modmsg = msg(Platform::Kick, "c", "a", "nul", 0);
        modmsg.author.is_moderator = true;
        assert!(a.evaluate(&modmsg).is_clean());
        r.enabled = false;
        let mut a = AutoMod::new(vec![r]);
        assert!(a.evaluate(&msg(Platform::Kick, "c", "a", "nul", 0)).is_clean());
    }

    #[test]
    fn score_sums_and_actions_combine() {
        let mut r1 = rule("liens", RuleKind::Links { allowed_domains: vec![] }, RuleAction::Delete);
        r1.score = 30;
        let mut r2 = rule("mots", RuleKind::BannedWords { words: vec!["arnaque".into()] }, RuleAction::Timeout { seconds: 600 });
        r2.score = 50;
        let mut a = AutoMod::new(vec![r1, r2]);
        let m = msg(Platform::Twitch, "c", "bob", "arnaque sur bit.ly/x", 5);
        let v = a.evaluate(&m);
        assert_eq!(v.score, 80);
        assert_eq!(v.reason().as_deref(), Some("liens + mots"));
        assert_eq!(
            v.moderation_kinds(&m),
            vec![
                ModerationKind::Timeout { user_id: "id-bob".into(), seconds: 600 },
                ModerationKind::DeleteMessage { message_id: "m-5".into() },
            ]
        );
        let flag_only = one(RuleKind::BannedWords { words: vec!["x".into()] }).preview(&msg(Platform::Twitch, "c", "b", "x", 1));
        assert!(!flag_only.is_clean());
        assert!(flag_only.moderation_kinds(&m).is_empty());
        let zero = rule("t", RuleKind::BannedWords { words: vec!["x".into()] }, RuleAction::Timeout { seconds: 0 });
        assert!(validate_rule(&zero).is_err());
    }

    #[test]
    fn longest_timeout_wins() {
        let words = || RuleKind::BannedWords { words: vec!["x".into()] };
        let a = AutoMod::new(vec![
            rule("a", words(), RuleAction::Timeout { seconds: 60 }),
            rule("b", words(), RuleAction::Timeout { seconds: 900 }),
        ]);
        assert_eq!(a.preview(&msg(Platform::Twitch, "c", "b", "x", 1)).timeout_secs(), Some(900));
    }

    #[test]
    fn rule_json_roundtrip_matches_migration_defaults() {
        let r = rule("r", RuleKind::Links { allowed_domains: vec!["a.fr".into()] }, RuleAction::Timeout { seconds: 30 });
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<Rule>(&json).unwrap(), r);
        let k: RuleKind = serde_json::from_str(r#"{"type":"cross_platform_spam","window_secs":60,"min_channels":2}"#).unwrap();
        assert_eq!(k, RuleKind::CrossPlatformSpam { window_secs: 60, min_channels: 2 });
    }
}
