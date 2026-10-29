//! Émotes tierces (7TV, BetterTTV, FrankerFaceZ), émotes natives Twitch et badges.
//!
//! Les fonctions d'analyse sont pures (JSON -> `Emote`) pour être testées sans réseau.
//! Le [`EmoteCatalog`] garde les émotes en mémoire et sur disque (un fichier JSON par canal,
//! réutilisé tant qu'il est récent ou si le réseau est indisponible).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::ChatMessage;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmoteProvider {
    Twitch,
    SevenTv,
    Bttv,
    Ffz,
}

/// Une émote connue (catalogue), identifiée par son code dans le chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emote {
    pub name: String,
    pub url: String,
    pub provider: EmoteProvider,
    #[serde(default)]
    pub zero_width: bool,
}

/// Position d'une émote dans le texte d'un message.
///
/// `start`/`end` sont des indices en caractères Unicode (`end` exclu), pas en octets ni en UTF-16:
/// côté interface, utiliser `Array.from(text)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmoteSpan {
    pub start: usize,
    pub end: usize,
    pub name: String,
    pub url: String,
    pub provider: EmoteProvider,
    #[serde(default)]
    pub zero_width: bool,
}

/// Badge d'un auteur (ex. `moderator` / `1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatBadge {
    pub set_id: String,
    pub version: String,
}

// ---------- Twitch natif ----------

/// Analyse le tag IRC `emotes` (`25:0-4,12-16/1902:6-10`). `offset` retire le préfixe `/me`
/// (les indices Twitch comptent le `\u{1}ACTION ` initial).
pub fn parse_twitch_emotes(tag: &str, text: &str, offset: usize) -> Vec<EmoteSpan> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    for entry in tag.split('/').filter(|e| !e.is_empty()) {
        let Some((id, ranges)) = entry.split_once(':') else {
            continue;
        };
        for range in ranges.split(',') {
            let Some((s, e)) = range.split_once('-') else {
                continue;
            };
            let (Ok(s), Ok(e)) = (s.parse::<usize>(), e.parse::<usize>()) else {
                continue;
            };
            let (Some(start), Some(end)) = (s.checked_sub(offset), (e + 1).checked_sub(offset))
            else {
                continue;
            };
            if start >= end || end > chars.len() {
                continue;
            }
            spans.push(EmoteSpan {
                start,
                end,
                name: chars[start..end].iter().collect(),
                url: format!("https://static-cdn.jtvnw.net/emoticons/v2/{id}/default/dark/2.0"),
                provider: EmoteProvider::Twitch,
                zero_width: false,
            });
        }
    }
    spans.sort_by_key(|s| s.start);
    spans
}

/// Analyse le tag IRC `badges` (`moderator/1,subscriber/12`).
pub fn parse_badges(tag: &str) -> Vec<ChatBadge> {
    tag.split(',')
        .filter_map(|b| {
            let (set, ver) = b.split_once('/')?;
            (!set.is_empty()).then(|| ChatBadge {
                set_id: set.into(),
                version: ver.into(),
            })
        })
        .collect()
}

// ---------- Réponses des API tierces ----------

fn abs_url(u: &str) -> String {
    if u.starts_with("//") {
        format!("https:{u}")
    } else {
        u.to_string()
    }
}

fn json(s: &str) -> Result<Value> {
    serde_json::from_str(s).map_err(|e| Error::Other(format!("réponse d'émotes invalide: {e}")))
}

fn parse_7tv_emote(e: &Value) -> Option<Emote> {
    let name = e.get("name")?.as_str()?.to_string();
    let data = e.get("data")?;
    let host = abs_url(data.get("host")?.get("url")?.as_str()?);
    let flags = data.get("flags").and_then(Value::as_u64).unwrap_or(0);
    Some(Emote {
        name,
        url: format!("{host}/2x.webp"),
        provider: EmoteProvider::SevenTv,
        zero_width: flags & 256 != 0,
    })
}

fn parse_7tv_list(list: Option<&Value>) -> Vec<Emote> {
    list.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(parse_7tv_emote).collect())
        .unwrap_or_default()
}

/// `GET https://7tv.io/v3/users/twitch/{id}` -> `emote_set.emotes`.
pub fn parse_7tv_user(body: &str) -> Result<Vec<Emote>> {
    Ok(parse_7tv_list(
        json(body)?.get("emote_set").and_then(|s| s.get("emotes")),
    ))
}

/// `GET https://7tv.io/v3/emote-sets/global` -> `emotes`.
pub fn parse_7tv_set(body: &str) -> Result<Vec<Emote>> {
    Ok(parse_7tv_list(json(body)?.get("emotes")))
}

fn bttv_emote(e: &Value) -> Option<Emote> {
    let id = e.get("id")?.as_str()?;
    let code = e.get("code")?.as_str()?;
    Some(Emote {
        name: code.into(),
        url: format!("https://cdn.betterttv.net/emote/{id}/2x.webp"),
        provider: EmoteProvider::Bttv,
        zero_width: false,
    })
}

/// `GET https://api.betterttv.net/3/cached/emotes/global` (tableau) ou
/// `/3/cached/users/twitch/{id}` (`channelEmotes` + `sharedEmotes`).
pub fn parse_bttv(body: &str) -> Result<Vec<Emote>> {
    let v = json(body)?;
    let lists: Vec<&Value> = if v.is_array() {
        vec![&v]
    } else {
        ["channelEmotes", "sharedEmotes"]
            .iter()
            .filter_map(|k| v.get(*k))
            .collect()
    };
    Ok(lists
        .into_iter()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(bttv_emote)
        .collect())
}

/// `GET https://api.frankerfacez.com/v1/room/id/{id}` ou `/v1/set/global`: `sets.*.emoticons`.
pub fn parse_ffz(body: &str) -> Result<Vec<Emote>> {
    let v = json(body)?;
    let mut out = Vec::new();
    for set in v
        .get("sets")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|m| m.values())
    {
        for e in set
            .get("emoticons")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(name), Some(urls)) = (
                e.get("name").and_then(Value::as_str),
                e.get("urls").and_then(Value::as_object),
            ) else {
                continue;
            };
            if let Some(url) = urls
                .get("2")
                .or_else(|| urls.get("1"))
                .and_then(Value::as_str)
            {
                out.push(Emote {
                    name: name.into(),
                    url: abs_url(url),
                    provider: EmoteProvider::Ffz,
                    zero_width: false,
                });
            }
        }
    }
    Ok(out)
}

// ---------- Annotation ----------

/// Repère dans `text` les mots correspondant à une émote du catalogue, hors zones déjà couvertes.
pub fn find_third_party(
    text: &str,
    existing: &[EmoteSpan],
    emotes: &HashMap<String, Emote>,
) -> Vec<EmoteSpan> {
    let mut out = Vec::new();
    let mut flush = |start: usize, end: usize, word: &str| {
        if existing.iter().any(|s| start < s.end && s.start < end) {
            return;
        }
        if let Some(e) = emotes.get(word) {
            out.push(EmoteSpan {
                start,
                end,
                name: e.name.clone(),
                url: e.url.clone(),
                provider: e.provider,
                zero_width: e.zero_width,
            });
        }
    };
    let mut word_start: Option<usize> = None;
    let mut word = String::new();
    let mut n = 0;
    for (i, c) in text.chars().enumerate() {
        n = i + 1;
        if c.is_whitespace() {
            if let Some(s) = word_start.take() {
                flush(s, i, &word);
                word.clear();
            }
        } else {
            word_start.get_or_insert(i);
            word.push(c);
        }
    }
    if let Some(s) = word_start {
        flush(s, n, &word);
    }
    out
}

// ---------- Catalogue ----------

const CACHE_TTL: Duration = Duration::from_secs(6 * 3600);
const GLOBAL_KEY: &str = "global";

/// Catalogue d'émotes tierces, partagé (clonable à bas coût).
#[derive(Clone)]
pub struct EmoteCatalog {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    dir: PathBuf,
    rooms: Mutex<HashMap<String, Arc<HashMap<String, Emote>>>>,
    loading: Mutex<HashSet<String>>,
}

impl EmoteCatalog {
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            inner: Arc::new(Inner {
                http,
                dir: cache_dir.into(),
                rooms: Mutex::new(HashMap::new()),
                loading: Mutex::new(HashSet::new()),
            }),
        }
    }

    /// Ajoute les émotes tierces (globales + du canal) au message.
    /// Ne bloque jamais: si le canal n'est pas encore chargé, le chargement démarre en tâche de fond
    /// (les premiers messages sont alors sans émotes tierces).
    /// À appeler depuis un runtime tokio.
    pub fn annotate(&self, msg: &mut ChatMessage) {
        let Some(room) = msg.room_id.clone() else {
            return;
        };
        self.ensure_loaded(GLOBAL_KEY);
        self.ensure_loaded(&room);
        for key in [room.as_str(), GLOBAL_KEY] {
            let map = self.inner.rooms.lock().unwrap().get(key).cloned();
            if let Some(map) = map {
                let found = find_third_party(&msg.text, &msg.emotes, &map);
                msg.emotes.extend(found);
            }
        }
        msg.emotes.sort_by_key(|s| s.start);
    }

    /// Lance (une seule fois) le chargement d'un canal (`global` ou identifiant Twitch).
    pub fn ensure_loaded(&self, key: &str) {
        if self.inner.rooms.lock().unwrap().contains_key(key)
            || !self.inner.loading.lock().unwrap().insert(key.to_string())
        {
            return;
        }
        let this = self.clone();
        let key = key.to_string();
        tokio::spawn(async move {
            match this.load(&key).await {
                Some(map) => {
                    this.inner
                        .rooms
                        .lock()
                        .unwrap()
                        .insert(key.clone(), Arc::new(map));
                }
                // Échec réseau sans cache: nouvel essai possible après une minute.
                None => tokio::time::sleep(Duration::from_secs(60)).await,
            }
            this.inner.loading.lock().unwrap().remove(&key);
        });
    }

    fn cache_path(&self, key: &str) -> PathBuf {
        let safe: String = key.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        self.inner.dir.join(format!("{safe}.json"))
    }

    fn read_cache(&self, key: &str, allow_stale: bool) -> Option<Vec<Emote>> {
        let path = self.cache_path(key);
        let age = SystemTime::now()
            .duration_since(std::fs::metadata(&path).ok()?.modified().ok()?)
            .unwrap_or_default();
        if !allow_stale && age > CACHE_TTL {
            return None;
        }
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    fn write_cache(&self, key: &str, emotes: &[Emote]) {
        if std::fs::create_dir_all(&self.inner.dir).is_ok() {
            if let Ok(body) = serde_json::to_string(emotes) {
                let _ = std::fs::write(self.cache_path(key), body);
            }
        }
    }

    async fn load(&self, key: &str) -> Option<HashMap<String, Emote>> {
        let list = match self.read_cache(key, false) {
            Some(l) => l,
            None => match self.fetch(key).await {
                Some(l) => {
                    self.write_cache(key, &l);
                    l
                }
                None => self.read_cache(key, true)?,
            },
        };
        // Priorité en cas de doublon: 7TV > BTTV > FFZ (le dernier inséré gagne).
        let mut map = HashMap::new();
        for p in [
            EmoteProvider::Ffz,
            EmoteProvider::Bttv,
            EmoteProvider::SevenTv,
        ] {
            for e in list.iter().filter(|e| e.provider == p) {
                map.insert(e.name.clone(), e.clone());
            }
        }
        Some(map)
    }

    /// `None` = échec (réseau, 5xx); un 404 (pas d'émotes pour ce canal) donne un corps vide.
    async fn get(&self, url: &str) -> Option<String> {
        let r = self.inner.http.get(url).send().await.ok()?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Some(String::new());
        }
        if !r.status().is_success() {
            return None;
        }
        r.text().await.ok()
    }

    /// Interroge les trois fournisseurs; `None` si l'un d'eux échoue (on n'écrit pas de cache partiel).
    async fn fetch(&self, key: &str) -> Option<Vec<Emote>> {
        let global = key == GLOBAL_KEY;
        let (u7, ub, uf) = if global {
            (
                "https://7tv.io/v3/emote-sets/global".to_string(),
                "https://api.betterttv.net/3/cached/emotes/global".to_string(),
                "https://api.frankerfacez.com/v1/set/global".to_string(),
            )
        } else {
            (
                format!("https://7tv.io/v3/users/twitch/{key}"),
                format!("https://api.betterttv.net/3/cached/users/twitch/{key}"),
                format!("https://api.frankerfacez.com/v1/room/id/{key}"),
            )
        };
        let (a, b, c) = tokio::join!(self.get(&u7), self.get(&ub), self.get(&uf));
        let mut out = Vec::new();
        for (body, parse) in [
            (
                a?,
                if global {
                    parse_7tv_set
                } else {
                    parse_7tv_user
                } as fn(&str) -> Result<Vec<Emote>>,
            ),
            (b?, parse_bttv),
            (c?, parse_ffz),
        ] {
            if !body.is_empty() {
                out.extend(parse(&body).ok()?);
            }
        }
        Some(out)
    }

    #[cfg(test)]
    fn insert_for_test(&self, key: &str, emotes: Vec<Emote>) {
        let map = emotes.into_iter().map(|e| (e.name.clone(), e)).collect();
        self.inner
            .rooms
            .lock()
            .unwrap()
            .insert(key.into(), Arc::new(map));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ChatAuthor, Platform};

    #[test]
    fn twitch_native_spans() {
        let s = parse_twitch_emotes("25:0-4,12-16/1902:6-10", "Kappa Keepo Kappa", 0);
        assert_eq!(s.len(), 3);
        assert_eq!((s[0].start, s[0].end, s[0].name.as_str()), (0, 5, "Kappa"));
        assert_eq!(s[1].name, "Keepo");
        assert!(s[1].url.contains("/1902/"));
    }

    #[test]
    fn twitch_spans_with_action_offset_and_garbage() {
        let s = parse_twitch_emotes("25:8-12", "Kappa", 8);
        assert_eq!(s[0].name, "Kappa");
        assert!(parse_twitch_emotes("x:9-1,25:0-99,bad", "Kappa", 0).is_empty());
    }

    #[test]
    fn twitch_spans_unicode() {
        let s = parse_twitch_emotes("25:3-7", "😀😀 Kappa", 0);
        assert_eq!(s[0].name, "Kappa");
    }

    #[test]
    fn badges() {
        let b = parse_badges("moderator/1,subscriber/12,,junk");
        assert_eq!(
            b,
            vec![
                ChatBadge {
                    set_id: "moderator".into(),
                    version: "1".into()
                },
                ChatBadge {
                    set_id: "subscriber".into(),
                    version: "12".into()
                },
            ]
        );
    }

    #[test]
    fn seventv_user_and_zero_width() {
        let body = r#"{"emote_set":{"emotes":[
            {"name":"OMEGALUL","data":{"flags":0,"host":{"url":"//cdn.7tv.app/emote/abc"}}},
            {"name":"cvMask","data":{"flags":256,"host":{"url":"//cdn.7tv.app/emote/def"}}},
            {"name":"broken"}]}}"#;
        let e = parse_7tv_user(body).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].url, "https://cdn.7tv.app/emote/abc/2x.webp");
        assert!(!e[0].zero_width && e[1].zero_width);
        assert!(parse_7tv_user(r#"{"emote_set":null}"#).unwrap().is_empty());
        assert!(parse_7tv_user("pas du json").is_err());
    }

    #[test]
    fn seventv_global_set() {
        let e = parse_7tv_set(
            r#"{"emotes":[{"name":"Clap","data":{"host":{"url":"//cdn.7tv.app/emote/x"}}}]}"#,
        )
        .unwrap();
        assert_eq!(e[0].name, "Clap");
    }

    #[test]
    fn bttv_user_and_global() {
        let user = r#"{"channelEmotes":[{"id":"1","code":"catJAM"}],"sharedEmotes":[{"id":"2","code":"widepeepo"}]}"#;
        let e = parse_bttv(user).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].url, "https://cdn.betterttv.net/emote/1/2x.webp");
        assert_eq!(
            parse_bttv(r#"[{"id":"9","code":"LUL2"}]"#).unwrap()[0].name,
            "LUL2"
        );
    }

    #[test]
    fn ffz_room() {
        let body = r#"{"sets":{"123":{"emoticons":[
            {"name":"ZrehplaR","urls":{"1":"//cdn.frankerfacez.com/a/1","2":"//cdn.frankerfacez.com/a/2"}},
            {"name":"NoUrl","urls":{}}]}}}"#;
        let e = parse_ffz(body).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].url, "https://cdn.frankerfacez.com/a/2");
        assert_eq!(e[0].provider, EmoteProvider::Ffz);
    }

    fn emote(name: &str, p: EmoteProvider) -> Emote {
        Emote {
            name: name.into(),
            url: format!("https://x/{name}"),
            provider: p,
            zero_width: false,
        }
    }

    #[test]
    fn finds_words_but_skips_native_overlap() {
        let map: HashMap<_, _> = [
            emote("catJAM", EmoteProvider::Bttv),
            emote("Kappa", EmoteProvider::SevenTv),
        ]
        .into_iter()
        .map(|e| (e.name.clone(), e))
        .collect();
        let native = parse_twitch_emotes("25:13-17", "salut, catJAM Kappa", 0);
        let found = find_third_party("salut, catJAM Kappa", &native, &map);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].start, found[0].end), (7, 13));
        assert_eq!(found[0].name, "catJAM");
        assert_eq!(find_third_party("xcatJAM catJAM", &[], &map).len(), 1);
    }

    #[tokio::test]
    async fn annotate_uses_loaded_catalog() {
        let dir =
            std::env::temp_dir().join(format!("omnistream-emotes-test-{}", std::process::id()));
        let cat = EmoteCatalog::new(&dir);
        cat.insert_for_test("42", vec![emote("catJAM", EmoteProvider::Bttv)]);
        cat.insert_for_test(GLOBAL_KEY, vec![]);
        let mut m = ChatMessage {
            id: "1".into(),
            channel_id: "c".into(),
            platform: Platform::Twitch,
            author: ChatAuthor {
                external_id: "1".into(),
                display_name: "a".into(),
                is_moderator: false,
                is_subscriber: false,
            },
            text: "yo catJAM".into(),
            timestamp_ms: 0,
            emotes: vec![],
            badges: vec![],
            room_id: Some("42".into()),
        };
        cat.annotate(&mut m);
        assert_eq!(m.emotes.len(), 1);
        assert_eq!(m.emotes[0].start, 3);
    }

    #[tokio::test]
    async fn disk_cache_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("omnistream-emotes-cache-{}", std::process::id()));
        let cat = EmoteCatalog::new(&dir);
        cat.write_cache("77", &[emote("a", EmoteProvider::Ffz)]);
        let map = cat.load("77").await.unwrap();
        assert!(map.contains_key("a"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
