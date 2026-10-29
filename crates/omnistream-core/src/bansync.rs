//! Bans synchronisés : propage un ban/unban d'une plateforme vers les autres chaînes.
//!
//! Au-dessus de [`ModerationEngine`] : table de correspondance d'identités,
//! prévisualisation, file d'attente persistante avec journal des échecs,
//! reprise et limitation de débit par connecteur.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::connectors::ConnectorRegistry;
use crate::db::Database;
use crate::domain::{Channel, ModerationAction, ModerationKind, Platform};
use crate::moderation::ModerationEngine;
use crate::{Error, Result};

/// Délai minimal par défaut entre deux appels vers un même connecteur.
pub const DEFAULT_MIN_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityLink {
    pub person: String,
    pub platform: Platform,
    pub user_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncKind {
    Ban,
    Unban,
}

impl SyncKind {
    fn as_str(self) -> &'static str {
        match self {
            SyncKind::Ban => "ban",
            SyncKind::Unban => "unban",
        }
    }
    /// `None` pour une valeur inconnue : on ne bannit jamais par défaut.
    fn parse(s: &str) -> Option<Self> {
        match s {
            "ban" => Some(SyncKind::Ban),
            "unban" => Some(SyncKind::Unban),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRequest {
    pub source_platform: Platform,
    pub source_user_id: String,
    pub source_name: String,
    pub kind: SyncKind,
    pub reason: Option<String>,
    /// Chaînes ciblées ; `None` = toutes les chaînes connues.
    pub channel_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    Ready,
    /// Aucune identité connue pour cet utilisateur sur la plateforme de la chaîne.
    NoIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewItem {
    pub channel: Channel,
    /// Comptes visés sur cette chaîne (une personne peut en avoir plusieurs sur une plateforme).
    pub user_ids: Vec<String>,
    pub status: PreviewStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: i64,
    pub channel_id: String,
    pub channel_name: String,
    pub platform: Platform,
    pub user_id: String,
    /// `pending`, `done` ou `failed`.
    pub status: String,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub kind: SyncKind,
    pub source_name: String,
    pub reason: Option<String>,
    pub created_at_ms: i64,
    pub items: Vec<QueueItem>,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub struct BanSync {
    db: Arc<Database>,
    engine: ModerationEngine,
    intervals: std::sync::Mutex<HashMap<Platform, Duration>>,
    default_interval: Duration,
    last_call: Mutex<HashMap<Platform, Instant>>,
    /// Sérialise les traitements de file (pas de double envoi).
    run_lock: Mutex<()>,
}

impl BanSync {
    pub fn new(db: Arc<Database>, connectors: ConnectorRegistry) -> Self {
        Self {
            db,
            engine: ModerationEngine::new(connectors),
            intervals: Default::default(),
            default_interval: DEFAULT_MIN_INTERVAL,
            last_call: Default::default(),
            run_lock: Default::default(),
        }
    }

    pub fn with_default_interval(mut self, d: Duration) -> Self {
        self.default_interval = d;
        self
    }

    pub fn set_min_interval(&self, platform: Platform, d: Duration) {
        self.intervals.lock().unwrap().insert(platform, d);
    }

    // ---- Correspondance d'identités -------------------------------------

    /// Associe un compte plateforme à une personne (la crée au besoin).
    /// Si le compte appartenait déjà à une autre personne, il est déplacé.
    pub fn link_identity(&self, link: &IdentityLink) -> Result<()> {
        if link.person.trim().is_empty() || link.user_id.trim().is_empty() {
            return Err(Error::Other("personne et identifiant requis".into()));
        }
        self.db.with_conn(|c| {
            c.execute(
                "INSERT INTO identity_links (person, platform, user_id, display_name) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(platform, user_id) DO UPDATE SET person = excluded.person, display_name = excluded.display_name",
                (link.person.trim(), link.platform.as_str(), link.user_id.trim(), &link.display_name),
            )?;
            Ok(())
        })
    }

    pub fn unlink_identity(&self, platform: &Platform, user_id: &str) -> Result<()> {
        self.db.with_conn(|c| {
            c.execute("DELETE FROM identity_links WHERE platform = ?1 AND user_id = ?2", (platform.as_str(), user_id))?;
            Ok(())
        })
    }

    pub fn list_identities(&self) -> Result<Vec<IdentityLink>> {
        self.db.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT person, platform, user_id, display_name FROM identity_links ORDER BY person, platform")?;
            let rows = stmt.query_map([], |r| {
                Ok(IdentityLink {
                    person: r.get(0)?,
                    platform: Platform::from(r.get::<_, String>(1)?.as_str()),
                    user_id: r.get(2)?,
                    display_name: r.get(3)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        })
    }

    /// Identifiant sur `target` de la personne qui possède (platform, user_id).
    fn resolve(&self, platform: &Platform, user_id: &str, target: &Platform) -> Result<Vec<String>> {
        self.db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT t.user_id FROM identity_links s JOIN identity_links t ON t.person = s.person
                 WHERE s.platform = ?1 AND s.user_id = ?2 AND t.platform = ?3 ORDER BY t.rowid",
            )?;
            let rows = stmt.query_map((platform.as_str(), user_id, target.as_str()), |r| r.get::<_, String>(0))?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        })
    }

    // ---- Prévisualisation ------------------------------------------------

    pub fn preview(&self, req: &SyncRequest) -> Result<Vec<PreviewItem>> {
        let mut items = Vec::new();
        for channel in self.db.list_channels()? {
            if let Some(ids) = &req.channel_ids {
                if !ids.contains(&channel.id) {
                    continue;
                }
            }
            let mut user_ids = if channel.platform == req.source_platform {
                vec![req.source_user_id.clone()]
            } else {
                self.resolve(&req.source_platform, &req.source_user_id, &channel.platform)?
            };
            // Les alias liés à la source sur sa propre plateforme sont bannis aussi.
            if channel.platform == req.source_platform {
                for id in self.resolve(&req.source_platform, &req.source_user_id, &channel.platform)? {
                    if !user_ids.contains(&id) {
                        user_ids.push(id);
                    }
                }
            }
            let status = if user_ids.is_empty() { PreviewStatus::NoIdentity } else { PreviewStatus::Ready };
            items.push(PreviewItem { channel, user_ids, status });
        }
        Ok(items)
    }

    // ---- File d'attente --------------------------------------------------

    /// Met en file les éléments prêts de la prévisualisation ; ne contacte aucune plateforme.
    pub fn enqueue(&self, req: &SyncRequest) -> Result<String> {
        let ready: Vec<PreviewItem> =
            self.preview(req)?.into_iter().filter(|i| i.status == PreviewStatus::Ready).collect();
        if ready.is_empty() {
            return Err(Error::Other("aucune chaîne cible avec une identité connue".into()));
        }
        let now = now_ms();
        self.db.with_conn(|c| {
            let n: i64 = c.query_row("SELECT COUNT(*) FROM ban_jobs", [], |r| r.get(0))?;
            let id = format!("bs-{now}-{n}");
            c.execute(
                "INSERT INTO ban_jobs (id, kind, source_name, reason, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                (&id, req.kind.as_str(), &req.source_name, &req.reason, now),
            )?;
            for i in &ready {
                for user_id in &i.user_ids {
                    c.execute(
                        "INSERT INTO ban_queue (job_id, channel_id, channel_name, platform, user_id, updated_at_ms)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        (&id, &i.channel.id, &i.channel.display_name, i.channel.platform.as_str(), user_id, now),
                    )?;
                }
            }
            Ok(id)
        })
    }

    /// Traite les éléments en attente (d'un job ou de tous), en respectant la limite de débit.
    pub async fn process(&self, job_id: Option<&str>) -> Result<()> {
        let _guard = self.run_lock.lock().await;
        // (id de file, id chaîne, ban/unban, utilisateur, motif)
        type Pending = (i64, String, Option<SyncKind>, String, Option<String>);
        let pending: Vec<Pending> =self.db.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT q.id, q.channel_id, j.kind, q.user_id, j.reason
                 FROM ban_queue q JOIN ban_jobs j ON j.id = q.job_id
                 WHERE q.status = 'pending' AND (?1 IS NULL OR q.job_id = ?1) ORDER BY q.id",
            )?;
            let rows = stmt.query_map([job_id], |r| {
                Ok((r.get(0)?, r.get(1)?, SyncKind::parse(&r.get::<_, String>(2)?), r.get(3)?, r.get(4)?))
            })?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        })?;

        let channels = self.db.list_channels()?;
        for (qid, channel_id, kind, user_id, reason) in pending {
            let Some(channel) = channels.iter().find(|c| c.id == channel_id).cloned() else {
                self.record(qid, Err("chaîne supprimée".into()))?;
                continue;
            };
            let kind = match kind {
                Some(SyncKind::Ban) => ModerationKind::Ban { user_id, reason },
                Some(SyncKind::Unban) => ModerationKind::Unban { user_id },
                None => {
                    self.record(qid, Err("type d'action inconnu".into()))?;
                    continue;
                }
            };
            self.throttle(&channel.platform).await;
            let result = self.engine.apply(&ModerationAction { channel, kind }).await.map_err(|e| e.to_string());
            self.record(qid, result)?;
        }
        Ok(())
    }

    async fn throttle(&self, platform: &Platform) {
        let interval = self.intervals.lock().unwrap().get(platform).copied().unwrap_or(self.default_interval);
        let wait = {
            let mut last = self.last_call.lock().await;
            let now = Instant::now();
            let at = match last.get(platform) {
                Some(prev) if *prev + interval > now => *prev + interval,
                _ => now,
            };
            last.insert(platform.clone(), at);
            at
        };
        tokio::time::sleep_until(wait).await;
    }

    fn record(&self, qid: i64, result: std::result::Result<(), String>) -> Result<()> {
        let (status, error) = match result {
            Ok(()) => ("done", None),
            Err(e) => ("failed", Some(e)),
        };
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE ban_queue SET status = ?1, last_error = ?2, attempts = attempts + 1, updated_at_ms = ?3 WHERE id = ?4",
                (status, error, now_ms(), qid),
            )?;
            Ok(())
        })
    }

    /// Remet les échecs en attente (d'un job ou de tous) puis les rejoue.
    pub async fn retry_failed(&self, job_id: Option<&str>) -> Result<()> {
        self.db.with_conn(|c| {
            c.execute(
                "UPDATE ban_queue SET status = 'pending' WHERE status = 'failed' AND (?1 IS NULL OR job_id = ?1)",
                [job_id],
            )?;
            Ok(())
        })?;
        self.process(job_id).await
    }

    /// Reprise après redémarrage : rejoue tout ce qui est resté en attente.
    pub async fn resume(&self) -> Result<()> {
        self.process(None).await
    }

    /// Journal : jobs les plus récents d'abord, avec le détail par chaîne.
    pub fn journal(&self, limit: u32) -> Result<Vec<Job>> {
        self.db.with_conn(|c| {
            let mut jobs: Vec<Job> = {
                let mut stmt = c.prepare(
                    "SELECT id, kind, source_name, reason, created_at_ms FROM ban_jobs ORDER BY created_at_ms DESC, rowid DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map([limit], |r| {
                    Ok(Job {
                        id: r.get(0)?,
                        kind: SyncKind::parse(&r.get::<_, String>(1)?).unwrap_or(SyncKind::Ban),
                        source_name: r.get(2)?,
                        reason: r.get(3)?,
                        created_at_ms: r.get(4)?,
                        items: Vec::new(),
                    })
                })?;
                rows.collect::<std::result::Result<_, _>>()?
            };
            let mut stmt = c.prepare(
                "SELECT id, channel_id, channel_name, platform, user_id, status, attempts, last_error, updated_at_ms
                 FROM ban_queue WHERE job_id = ?1 ORDER BY id",
            )?;
            for job in &mut jobs {
                let rows = stmt.query_map([&job.id], |r| {
                    Ok(QueueItem {
                        id: r.get(0)?,
                        channel_id: r.get(1)?,
                        channel_name: r.get(2)?,
                        platform: Platform::from(r.get::<_, String>(3)?.as_str()),
                        user_id: r.get(4)?,
                        status: r.get(5)?,
                        attempts: r.get(6)?,
                        last_error: r.get(7)?,
                        updated_at_ms: r.get(8)?,
                    })
                })?;
                job.items = rows.collect::<std::result::Result<_, _>>()?;
            }
            Ok(jobs)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use super::*;
    use crate::connectors::{ChatConnector, ChatEvent};

    /// Connecteur factice : enregistre les actions, échoue tant que `fail` est vrai.
    struct Recorder {
        platform: Platform,
        calls: StdMutex<Vec<(String, Instant)>>,
        fail: StdMutex<bool>,
    }

    impl Recorder {
        fn new(platform: Platform) -> Arc<Self> {
            Arc::new(Self { platform, calls: Default::default(), fail: Default::default() })
        }
    }

    #[async_trait]
    impl ChatConnector for Recorder {
        fn platform(&self) -> Platform {
            self.platform.clone()
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
            if *self.fail.lock().unwrap() {
                return Err(Error::Network("429".into()));
            }
            let what = match &a.kind {
                ModerationKind::Ban { user_id, .. } => format!("ban:{user_id}"),
                ModerationKind::Unban { user_id } => format!("unban:{user_id}"),
                _ => "autre".into(),
            };
            self.calls.lock().unwrap().push((what, Instant::now()));
            Ok(())
        }
    }

    fn setup(interval: Duration) -> (BanSync, Arc<Recorder>, Arc<Recorder>) {
        let db = Arc::new(Database::open_in_memory().unwrap());
        for (id, p, ext) in [
            ("twitch:a", Platform::Twitch, "a"),
            ("twitch:b", Platform::Twitch, "b"),
            ("kick:a", Platform::Kick, "a"),
            ("yt:a", Platform::Youtube, "a"),
        ] {
            db.save_channel(&Channel { id: id.into(), platform: p, external_id: ext.into(), display_name: ext.into() }).unwrap();
        }
        let tw = Recorder::new(Platform::Twitch);
        let kick = Recorder::new(Platform::Kick);
        let mut reg = ConnectorRegistry::new();
        reg.register(tw.clone());
        reg.register(kick.clone());
        let bs = BanSync::new(db, reg).with_default_interval(interval);
        bs.link_identity(&IdentityLink { person: "troll".into(), platform: Platform::Twitch, user_id: "t1".into(), display_name: "Troll".into() }).unwrap();
        bs.link_identity(&IdentityLink { person: "troll".into(), platform: Platform::Kick, user_id: "k1".into(), display_name: "Troll".into() }).unwrap();
        (bs, tw, kick)
    }

    fn req() -> SyncRequest {
        SyncRequest {
            source_platform: Platform::Twitch,
            source_user_id: "t1".into(),
            source_name: "Troll".into(),
            kind: SyncKind::Ban,
            reason: Some("spam".into()),
            channel_ids: None,
        }
    }

    #[test]
    fn preview_maps_identities_and_flags_unknown_platforms() {
        let (bs, _, _) = setup(Duration::ZERO);
        let p = bs.preview(&req()).unwrap();
        let get = |id: &str| p.iter().find(|i| i.channel.id == id).unwrap();
        assert_eq!(get("twitch:a").user_ids, vec!["t1"]);
        assert_eq!(get("twitch:b").user_ids, vec!["t1"]);
        assert_eq!(get("kick:a").user_ids, vec!["k1"]);
        assert_eq!(get("yt:a").status, PreviewStatus::NoIdentity);

        let mut r = req();
        r.channel_ids = Some(vec!["kick:a".into()]);
        assert_eq!(bs.preview(&r).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn enqueue_process_dispatches_with_mapped_ids() {
        let (bs, tw, kick) = setup(Duration::ZERO);
        let job = bs.enqueue(&req()).unwrap();
        // Rien n'est envoyé avant le traitement.
        assert!(kick.calls.lock().unwrap().is_empty());
        bs.process(Some(&job)).await.unwrap();
        assert_eq!(kick.calls.lock().unwrap()[0].0, "ban:k1");
        assert_eq!(tw.calls.lock().unwrap().len(), 2);
        let j = &bs.journal(10).unwrap()[0];
        assert_eq!(j.items.len(), 3);
        assert!(j.items.iter().all(|i| i.status == "done" && i.attempts == 1));
        // Idempotent : un second passage ne renvoie rien.
        bs.process(None).await.unwrap();
        assert_eq!(kick.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn failures_are_journaled_and_retried() {
        let (bs, _, kick) = setup(Duration::ZERO);
        *kick.fail.lock().unwrap() = true;
        let job = bs.enqueue(&req()).unwrap();
        bs.process(Some(&job)).await.unwrap();
        let j = &bs.journal(10).unwrap()[0];
        let failed: Vec<_> = j.items.iter().filter(|i| i.status == "failed").collect();
        assert_eq!(failed.len(), 1);
        assert!(failed[0].last_error.as_deref().unwrap().contains("429"));

        *kick.fail.lock().unwrap() = false;
        bs.retry_failed(Some(&job)).await.unwrap();
        let j = &bs.journal(10).unwrap()[0];
        assert!(j.items.iter().all(|i| i.status == "done"));
        assert_eq!(j.items.iter().find(|i| i.channel_id == "kick:a").unwrap().attempts, 2);
        assert_eq!(kick.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rate_limits_per_connector() {
        let (bs, tw, kick) = setup(Duration::from_millis(150));
        let job = bs.enqueue(&req()).unwrap();
        bs.process(Some(&job)).await.unwrap();
        let calls = tw.calls.lock().unwrap();
        assert!(calls[1].1 - calls[0].1 >= Duration::from_millis(50));
        assert_eq!(kick.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn bans_every_linked_account_on_a_platform() {
        let (bs, _, kick) = setup(Duration::ZERO);
        bs.link_identity(&IdentityLink { person: "troll".into(), platform: Platform::Kick, user_id: "k2".into(), display_name: "alt".into() }).unwrap();
        let job = bs.enqueue(&req()).unwrap();
        bs.process(Some(&job)).await.unwrap();
        let calls: Vec<String> = kick.calls.lock().unwrap().iter().map(|c| c.0.clone()).collect();
        assert_eq!(calls, vec!["ban:k1", "ban:k2"]);
    }

    #[test]
    fn enqueue_without_target_errors_and_relink_moves_identity() {
        let (bs, _, _) = setup(Duration::ZERO);
        let mut r = req();
        r.source_platform = Platform::Youtube;
        r.source_user_id = "inconnu".into();
        r.channel_ids = Some(vec!["twitch:a".into()]);
        assert!(bs.enqueue(&r).is_err());

        bs.link_identity(&IdentityLink { person: "autre".into(), platform: Platform::Kick, user_id: "k1".into(), display_name: "x".into() }).unwrap();
        let p = bs.preview(&req()).unwrap();
        assert_eq!(p.iter().find(|i| i.channel.id == "kick:a").unwrap().status, PreviewStatus::NoIdentity);
        assert_eq!(bs.list_identities().unwrap().len(), 2);
    }
}
