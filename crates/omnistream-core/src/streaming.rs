//! Diffusion multi-plateforme : un ingest RTMP local (voir [`crate::ingest`]) et,
//! pour chaque destination activée, un processus FFmpeg supervisé qui relaie le
//! flux sans ré-encodage (`-c copy`). Une destination qui plante n'interrompt
//! pas les autres et est relancée avec un délai croissant.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{broadcast, watch, Mutex};
use tokio::task::JoinHandle;

use crate::db::Database;
use crate::domain::{Platform, StreamDestination};
use crate::ffmpeg::{self, FfmpegStatus};
use crate::ingest::{Ingest, IngestState, IngestStatus};
use crate::secrets::SecretStore;
use crate::{Error, Result};

pub const DEFAULT_INGEST_PORT: u16 = 1935;
/// Après autant d'échecs consécutifs sans jamais avoir diffusé, on abandonne la destination.
const MAX_ATTEMPTS: u32 = 8;

const SETTING_PORT: &str = "ingest_port";
const SETTING_KEY: &str = "ingest_key";
const SETTING_FFMPEG: &str = "ffmpeg_path";
const SETTING_TITLE: &str = "stream_title";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DestState {
    /// Pas de diffusion en cours.
    Idle,
    /// Diffusion lancée, en attente du signal de l'encodeur (OBS).
    WaitingSignal,
    Connecting,
    Live,
    /// Échec : nouvelle tentative programmée.
    Retrying,
    /// Abandon (clé manquante, trop d'échecs…).
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct DestinationStatus {
    pub id: String,
    pub state: DestState,
    pub error: Option<String>,
    pub attempt: u32,
    pub bitrate_kbps: Option<f64>,
}

impl DestinationStatus {
    fn idle(id: &str) -> Self {
        Self { id: id.into(), state: DestState::Idle, error: None, attempt: 0, bitrate_kbps: None }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum StreamingEvent {
    Ingest(IngestStatus),
    Destination(DestinationStatus),
    /// La diffusion vers les destinations a été lancée / arrêtée.
    Live(bool),
}

/// Destination telle qu'exposée à l'interface : la clé n'en sort jamais.
#[derive(Debug, Clone, Serialize)]
pub struct DestinationView {
    pub id: String,
    pub platform: Platform,
    pub name: String,
    pub rtmp_url: String,
    pub has_key: bool,
    pub enabled: bool,
    pub status: DestinationStatus,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DestinationInput {
    pub id: Option<String>,
    pub platform: Platform,
    pub name: String,
    pub rtmp_url: String,
    /// `None` conserve la clé existante.
    pub stream_key: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestInfo {
    pub url: String,
    pub key: String,
    pub port: u16,
    pub status: IngestStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamingState {
    pub live: bool,
    pub ffmpeg: FfmpegStatus,
    pub ingest: IngestInfo,
    pub title: String,
}

struct Inner {
    db: Arc<Database>,
    secrets: Arc<dyn SecretStore>,
    events: broadcast::Sender<StreamingEvent>,
    ingest: Mutex<Option<Ingest>>,
    last_ingest_error: StdMutex<Option<String>>,
    tasks: StdMutex<HashMap<String, JoinHandle<()>>>,
    statuses: StdMutex<HashMap<String, DestinationStatus>>,
    live: AtomicBool,
}

#[derive(Clone)]
pub struct StreamService {
    inner: Arc<Inner>,
}

fn secret_name(id: &str) -> String {
    format!("destination:{id}")
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::getrandom(&mut buf).expect("générateur aléatoire du système indisponible");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

impl StreamService {
    pub fn new(db: Arc<Database>, secrets: Arc<dyn SecretStore>) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(Inner {
                db,
                secrets,
                events,
                ingest: Mutex::new(None),
                last_ingest_error: StdMutex::default(),
                tasks: StdMutex::default(),
                statuses: StdMutex::default(),
                live: AtomicBool::new(false),
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StreamingEvent> {
        self.inner.events.subscribe()
    }

    // ---------- Ingest ----------

    fn ingest_port(&self) -> u16 {
        self.inner
            .db
            .get_setting(SETTING_PORT)
            .ok()
            .flatten()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_INGEST_PORT)
    }

    fn ingest_key(&self) -> Result<String> {
        if let Some(k) = self.inner.db.get_setting(SETTING_KEY)?.filter(|k| !k.is_empty()) {
            return Ok(k);
        }
        let key = format!("omni_{}", random_hex(8));
        self.inner.db.set_setting(SETTING_KEY, &key)?;
        Ok(key)
    }

    /// (Re)démarre le serveur d'ingest ; l'échec éventuel est visible dans son statut.
    pub async fn start_ingest(&self) -> Result<()> {
        let mut slot = self.inner.ingest.lock().await;
        *slot = None; // libère le port avant de le rouvrir
        let started = Ingest::start(self.ingest_port(), self.ingest_key()?).await;
        match started {
            Ok(ingest) => {
                *self.inner.last_ingest_error.lock().unwrap() = None;
                let mut rx = ingest.status();
                *slot = Some(ingest);
                let inner = self.inner.clone();
                let _ = inner.events.send(StreamingEvent::Ingest(rx.borrow().clone()));
                tokio::spawn(async move {
                    while rx.changed().await.is_ok() {
                        let status = rx.borrow_and_update().clone();
                        let _ = inner.events.send(StreamingEvent::Ingest(status));
                    }
                });
                Ok(())
            }
            Err(msg) => {
                *self.inner.last_ingest_error.lock().unwrap() = Some(msg.clone());
                let _ = self.inner.events.send(StreamingEvent::Ingest(self.error_status(msg.clone())));
                Err(Error::Other(msg))
            }
        }
    }

    fn error_status(&self, msg: String) -> IngestStatus {
        IngestStatus { state: IngestState::Error, error: Some(msg), ..Default::default() }
    }

    pub async fn set_ingest_port(&self, port: u16) -> Result<()> {
        if port < 1024 {
            return Err(Error::Other("Choisissez un port supérieur à 1024.".into()));
        }
        self.inner.db.set_setting(SETTING_PORT, &port.to_string())?;
        self.start_ingest().await
    }

    pub async fn ingest_info(&self) -> Result<IngestInfo> {
        let slot = self.inner.ingest.lock().await;
        Ok(match slot.as_ref() {
            Some(i) => IngestInfo { url: i.url(), key: i.key().into(), port: i.port(), status: i.status().borrow().clone() },
            None => {
                let port = self.ingest_port();
                let error = self.inner.last_ingest_error.lock().unwrap().clone();
                IngestInfo {
                    url: format!("rtmp://127.0.0.1:{port}/{}", crate::ingest::INGEST_APP),
                    key: self.ingest_key()?,
                    port,
                    status: error.map(|m| self.error_status(m)).unwrap_or_default(),
                }
            }
        })
    }

    // ---------- FFmpeg ----------

    fn ffmpeg_custom_path(&self) -> Option<String> {
        self.inner.db.get_setting(SETTING_FFMPEG).ok().flatten().filter(|p| !p.is_empty())
    }

    pub async fn ffmpeg_status(&self) -> FfmpegStatus {
        ffmpeg::detect(self.ffmpeg_custom_path().as_deref()).await
    }

    pub async fn set_ffmpeg_path(&self, path: &str) -> Result<FfmpegStatus> {
        self.inner.db.set_setting(SETTING_FFMPEG, path.trim())?;
        Ok(self.ffmpeg_status().await)
    }

    // ---------- Destinations ----------

    fn resolve_key(&self, d: &StreamDestination) -> Option<String> {
        if !d.stream_key.is_empty() {
            return Some(d.stream_key.clone());
        }
        self.inner.secrets.get(&secret_name(&d.id)).ok().flatten().filter(|k| !k.is_empty())
    }

    fn status_of(&self, id: &str) -> DestinationStatus {
        self.inner.statuses.lock().unwrap().get(id).cloned().unwrap_or_else(|| DestinationStatus::idle(id))
    }

    fn view(&self, d: StreamDestination) -> DestinationView {
        DestinationView {
            has_key: self.resolve_key(&d).is_some(),
            status: self.status_of(&d.id),
            id: d.id,
            platform: d.platform,
            name: d.name,
            rtmp_url: d.rtmp_url,
            enabled: d.enabled,
        }
    }

    pub fn list_destinations(&self) -> Result<Vec<DestinationView>> {
        Ok(self.inner.db.list_destinations()?.into_iter().map(|d| self.view(d)).collect())
    }

    pub async fn save_destination(&self, input: DestinationInput) -> Result<DestinationView> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err(Error::Other("Donnez un nom à la destination.".into()));
        }
        let url = input.rtmp_url.trim().to_string();
        if !(url.starts_with("rtmp://") || url.starts_with("rtmps://")) || url.len() < 10 {
            return Err(Error::Other("L'adresse du serveur doit commencer par rtmp:// ou rtmps://.".into()));
        }
        let id = input.id.filter(|i| !i.is_empty()).unwrap_or_else(|| format!("dest-{}", random_hex(6)));

        let mut dest = StreamDestination {
            id: id.clone(),
            platform: input.platform,
            name,
            rtmp_url: url,
            stream_key: String::new(),
            enabled: input.enabled,
        };
        match input.stream_key.map(|k| k.trim().to_string()) {
            Some(key) if !key.is_empty() => match self.inner.secrets.set(&secret_name(&id), &key) {
                Ok(()) => {}
                Err(e) => {
                    // Pas de trousseau (ex. session sans service de secrets) : repli en base locale.
                    eprintln!("clé de stream stockée dans la base locale: {e}");
                    dest.stream_key = key;
                }
            },
            // Clé inchangée : on conserve ce qui est déjà en base (repli éventuel).
            _ => {
                if let Some(old) = self.inner.db.list_destinations()?.into_iter().find(|d| d.id == id) {
                    dest.stream_key = old.stream_key;
                }
            }
        }
        self.inner.db.save_destination(&dest)?;
        self.sync_destination(&id).await;
        Ok(self.view(dest))
    }

    pub async fn delete_destination(&self, id: &str) -> Result<()> {
        self.stop_task(id);
        let _ = self.inner.secrets.delete(&secret_name(id));
        self.inner.statuses.lock().unwrap().remove(id);
        self.inner.db.delete_destination(id)
    }

    /// Applique un changement de destination à la diffusion en cours.
    async fn sync_destination(&self, id: &str) {
        if !self.inner.live.load(Ordering::Relaxed) {
            return;
        }
        self.stop_task(id);
        if let Ok(Some(d)) = self.inner.db.get_destination(id) {
            if d.enabled {
                let _ = self.spawn_supervisor(d).await;
            }
        }
    }

    // ---------- Diffusion ----------

    pub fn is_live(&self) -> bool {
        self.inner.live.load(Ordering::Relaxed)
    }

    pub async fn go_live(&self) -> Result<()> {
        let ffmpeg = self.ffmpeg_status().await;
        if !ffmpeg.found {
            return Err(Error::Other(ffmpeg.message.unwrap_or_default()));
        }
        if self.inner.ingest.lock().await.is_none() {
            let msg = self.inner.last_ingest_error.lock().unwrap().clone();
            return Err(Error::Other(msg.unwrap_or_else(|| "Le serveur d'entrée n'est pas démarré.".into())));
        }
        let enabled: Vec<_> = self.inner.db.list_destinations()?.into_iter().filter(|d| d.enabled).collect();
        if enabled.is_empty() {
            return Err(Error::Other("Activez au moins une destination avant de passer en direct.".into()));
        }
        self.inner.live.store(true, Ordering::Relaxed);
        let _ = self.inner.events.send(StreamingEvent::Live(true));
        for d in enabled {
            self.stop_task(&d.id);
            self.spawn_supervisor(d).await?;
        }
        Ok(())
    }

    pub async fn stop_live(&self) {
        self.inner.live.store(false, Ordering::Relaxed);
        let ids: Vec<String> = self.inner.tasks.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.stop_task(&id);
        }
        let _ = self.inner.events.send(StreamingEvent::Live(false));
    }

    fn stop_task(&self, id: &str) {
        if let Some(task) = self.inner.tasks.lock().unwrap().remove(id) {
            task.abort(); // `kill_on_drop` arrête le processus FFmpeg
        }
        self.inner.report(DestinationStatus::idle(id));
    }

    async fn spawn_supervisor(&self, dest: StreamDestination) -> Result<()> {
        let ffmpeg = self.ffmpeg_status().await;
        let Some(path) = ffmpeg.path.filter(|_| ffmpeg.found) else {
            return Err(Error::Other(ffmpeg.message.unwrap_or_default()));
        };
        let Some(key) = self.resolve_key(&dest) else {
            self.inner.report(DestinationStatus {
                state: DestState::Error,
                error: Some("Clé de stream manquante : renseignez-la pour diffuser sur cette destination.".into()),
                ..DestinationStatus::idle(&dest.id)
            });
            return Ok(());
        };
        let (play_url, ingest_rx) = {
            let slot = self.inner.ingest.lock().await;
            let ingest = slot.as_ref().ok_or_else(|| Error::Other("Le serveur d'entrée n'est pas démarré.".into()))?;
            (ingest.play_url(), ingest.status())
        };
        let job = Job { id: dest.id.clone(), url: dest.rtmp_url.clone(), key, ffmpeg: path, play_url };
        let inner = self.inner.clone();
        let task = tokio::spawn(async move { supervise(inner, job, ingest_rx).await });
        self.inner.tasks.lock().unwrap().insert(dest.id, task);
        Ok(())
    }

    // ---------- Titre ----------

    pub fn title(&self) -> String {
        self.inner.db.get_setting(SETTING_TITLE).ok().flatten().unwrap_or_default()
    }

    pub fn save_title(&self, title: &str) -> Result<()> {
        self.inner.db.set_setting(SETTING_TITLE, title.trim())
    }

    pub async fn state(&self) -> Result<StreamingState> {
        Ok(StreamingState {
            live: self.is_live(),
            ffmpeg: self.ffmpeg_status().await,
            ingest: self.ingest_info().await?,
            title: self.title(),
        })
    }
}

impl Inner {
    fn report(&self, status: DestinationStatus) {
        self.statuses.lock().unwrap().insert(status.id.clone(), status.clone());
        let _ = self.events.send(StreamingEvent::Destination(status));
    }
}

struct Job {
    id: String,
    url: String,
    key: String,
    ffmpeg: String,
    play_url: String,
}

fn ffmpeg_args(play_url: &str, out_url: &str, key: &str) -> Vec<String> {
    let full = format!("{}/{}", out_url.trim_end_matches('/'), key);
    [
        "-hide_banner", "-loglevel", "warning", "-nostats", "-progress", "pipe:1",
        "-i", play_url, "-c", "copy", "-f", "flv", "-flvflags", "no_duration_filesize", &full,
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Retire la clé de stream (et l'URL complète) d'un message avant affichage.
fn redact(line: &str, key: &str) -> String {
    if key.is_empty() {
        return line.to_string();
    }
    line.replace(key, "***")
}

fn friendly_error(line: &str) -> String {
    let l = line.to_lowercase();
    let hint = if l.contains("connection refused") {
        "connexion refusée par le serveur"
    } else if l.contains("403") || l.contains("401") || l.contains("unauthorized") || l.contains("forbidden") {
        "accès refusé : vérifiez la clé de stream"
    } else if l.contains("could not resolve") || l.contains("name or service not known") || l.contains("no such host") {
        "serveur introuvable : vérifiez l'adresse RTMP"
    } else if l.contains("timed out") || l.contains("timeout") {
        "le serveur ne répond pas"
    } else if l.contains("broken pipe") || l.contains("connection reset") {
        "connexion interrompue par le serveur"
    } else {
        return line.to_string();
    };
    format!("{hint} ({line})")
}

enum RunOutcome {
    Failed { error: String, went_live: bool },
}

async fn run_ffmpeg(inner: &Arc<Inner>, job: &Job) -> RunOutcome {
    let fail = |error: String, went_live| RunOutcome::Failed { error, went_live };
    let mut cmd = ffmpeg::command(&job.ffmpeg);
    cmd.args(ffmpeg_args(&job.play_url, &job.url, &job.key))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return fail(format!("Impossible de lancer FFmpeg : {e}"), false),
    };

    let stderr = child.stderr.take().expect("stderr capturée");
    let last_lines: Arc<StdMutex<Vec<String>>> = Arc::default();
    let sink = last_lines.clone();
    let key = job.key.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = redact(line.trim(), &key);
            if line.is_empty() {
                continue;
            }
            let mut v = sink.lock().unwrap();
            v.push(line);
            if v.len() > 4 {
                v.remove(0);
            }
        }
    });

    let stdout = child.stdout.take().expect("stdout capturée");
    let mut lines = BufReader::new(stdout).lines();
    let mut went_live = false;
    let mut bitrate = None;
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(v) = line.strip_prefix("bitrate=") {
            bitrate = v.trim().trim_end_matches("kbits/s").trim().parse::<f64>().ok();
        } else if line.starts_with("progress=") {
            // Le premier bloc de progression n'arrive qu'une fois connecté à la destination.
            if !went_live {
                went_live = true;
            }
            inner.report(DestinationStatus {
                state: DestState::Live,
                bitrate_kbps: bitrate,
                ..DestinationStatus::idle(&job.id)
            });
        }
    }
    let status = child.wait().await;
    let tail = last_lines.lock().unwrap().last().cloned();
    let error = match (tail, status) {
        (Some(line), _) => friendly_error(&line),
        (None, Ok(s)) => format!("FFmpeg s'est arrêté (code {}).", s.code().map_or("?".into(), |c| c.to_string())),
        (None, Err(e)) => format!("FFmpeg s'est arrêté : {e}"),
    };
    fail(error, went_live)
}

async fn supervise(inner: Arc<Inner>, job: Job, mut ingest: watch::Receiver<IngestStatus>) {
    let mut attempt = 0u32;
    loop {
        inner.report(DestinationStatus { state: DestState::WaitingSignal, ..DestinationStatus::idle(&job.id) });
        if ingest.wait_for(|s| s.state == IngestState::Live).await.is_err() {
            return;
        }
        inner.report(DestinationStatus { state: DestState::Connecting, attempt, ..DestinationStatus::idle(&job.id) });

        let started = Instant::now();
        let RunOutcome::Failed { error, went_live } = run_ffmpeg(&inner, &job).await;
        if went_live || started.elapsed() > Duration::from_secs(30) {
            attempt = 0;
        }
        // L'encodeur s'est arrêté : ce n'est pas la faute de la destination.
        if ingest.borrow().state != IngestState::Live {
            continue;
        }
        attempt += 1;
        if attempt >= MAX_ATTEMPTS {
            inner.report(DestinationStatus {
                state: DestState::Error,
                error: Some(format!("Abandon après {MAX_ATTEMPTS} tentatives : {error}")),
                attempt,
                ..DestinationStatus::idle(&job.id)
            });
            return;
        }
        inner.report(DestinationStatus {
            state: DestState::Retrying,
            error: Some(error),
            attempt,
            ..DestinationStatus::idle(&job.id)
        });
        tokio::time::sleep(Duration::from_secs((1u64 << attempt.min(5)).min(30))).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;
    use std::ops::Not;

    fn service() -> StreamService {
        StreamService::new(Arc::new(Database::open_in_memory().unwrap()), Arc::new(MemoryStore::default()))
    }

    fn input(key: Option<&str>) -> DestinationInput {
        DestinationInput {
            id: None,
            platform: Platform::Twitch,
            name: "Twitch".into(),
            rtmp_url: "rtmp://live.twitch.tv/app/".into(),
            stream_key: key.map(Into::into),
            enabled: true,
        }
    }

    #[test]
    fn builds_copy_args_for_destination() {
        let args = ffmpeg_args("rtmp://127.0.0.1:1935/live/k", "rtmp://live.twitch.tv/app/", "KEY");
        assert_eq!(args.last().unwrap(), "rtmp://live.twitch.tv/app/KEY");
        assert!(args.windows(2).any(|w| w == ["-c", "copy"]));
    }

    #[test]
    fn redacts_stream_key() {
        assert_eq!(redact("failed rtmp://x/app/SECRET", "SECRET"), "failed rtmp://x/app/***");
    }

    #[tokio::test]
    async fn key_goes_to_secret_store_not_database() {
        let svc = service();
        let view = svc.save_destination(input(Some("live_abc"))).await.unwrap();
        assert!(view.has_key);
        let stored = svc.inner.db.list_destinations().unwrap();
        assert_eq!(stored[0].stream_key, "");
        // Une sauvegarde sans clé conserve la clé existante.
        let again = svc.save_destination(DestinationInput { id: Some(view.id.clone()), ..input(None) }).await.unwrap();
        assert!(again.has_key);
        svc.delete_destination(&view.id).await.unwrap();
        assert!(svc.list_destinations().unwrap().is_empty());
    }

    /// Bout en bout avec un vrai FFmpeg : un encodeur publie dans l'ingest, qui
    /// est relayé vers une « plateforme » (un second serveur RTMP local).
    #[tokio::test(flavor = "multi_thread")]
    async fn relays_encoder_stream_to_destination() {
        if ffmpeg::detect(None).await.found.not() {
            eprintln!("ffmpeg absent : test ignoré");
            return;
        }
        let svc = service();
        svc.inner.db.set_setting(SETTING_PORT, "0").unwrap();
        svc.start_ingest().await.unwrap();
        let info = svc.ingest_info().await.unwrap();

        let sink = Ingest::start(0, "sinkkey").await.unwrap();
        let mut sink_status = sink.status();
        svc.save_destination(DestinationInput {
            rtmp_url: sink.url(),
            ..input(Some("sinkkey"))
        })
        .await
        .unwrap();
        svc.go_live().await.unwrap();

        let mut encoder = ffmpeg::command("ffmpeg")
            .args(["-loglevel", "error", "-re", "-f", "lavfi", "-i", "testsrc=size=1280x720:rate=30"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440", "-c:v", "libx264", "-preset", "ultrafast"])
            .args(["-g", "30", "-b:v", "2000k", "-c:a", "aac", "-f", "flv"])
            .arg(format!("{}/{}", info.url, info.key))
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        // Ne pas garder la `Ref` renvoyée : elle bloquerait les mises à jour du statut.
        let relayed = tokio::time::timeout(Duration::from_secs(25), sink_status.wait_for(|s| s.state == IngestState::Live))
            .await
            .is_ok_and(|r| r.is_ok());
        assert!(relayed, "le flux n'est pas arrivé à la destination");
        tokio::time::sleep(Duration::from_secs(3)).await;

        let ingest = svc.ingest_info().await.unwrap().status;
        assert_eq!((ingest.width, ingest.height), (Some(1280), Some(720)));
        assert_eq!(ingest.video_codec.as_deref(), Some("H.264"));
        assert!(ingest.bitrate_kbps.unwrap_or(0) > 100, "débit: {:?}", ingest.bitrate_kbps);
        assert!(ingest.fps.unwrap_or(0.0) >= 25.0, "fps: {:?}", ingest.fps);
        let dest = svc.list_destinations().unwrap().remove(0);
        assert_eq!(dest.status.state, DestState::Live);

        // L'arrêt de l'encodeur remet la destination en attente de signal, sans erreur.
        encoder.kill().await.unwrap();
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(svc.ingest_info().await.unwrap().status.state, IngestState::Waiting);
        assert_eq!(svc.list_destinations().unwrap().remove(0).status.state, DestState::WaitingSignal);

        svc.stop_live().await;
        assert_eq!(svc.list_destinations().unwrap().remove(0).status.state, DestState::Idle);
    }

    #[tokio::test]
    async fn rejects_invalid_destination() {
        let svc = service();
        let bad = DestinationInput { rtmp_url: "http://nope".into(), ..input(None) };
        assert!(svc.save_destination(bad).await.is_err());
        let unnamed = DestinationInput { name: "  ".into(), ..input(None) };
        assert!(svc.save_destination(unnamed).await.is_err());
    }
}
