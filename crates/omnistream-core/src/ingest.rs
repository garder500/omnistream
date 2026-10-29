//! Serveur RTMP local intégré : OBS (ou tout encodeur) y publie, et chaque
//! destination FFmpeg vient y lire le flux (`-c copy`), sans ré-encodage.
//!
//! Le serveur mesure aussi le signal reçu (résolution, images/s, débit).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{
    ServerSession, ServerSessionConfig, ServerSessionEvent, ServerSessionResult, StreamMetadata,
};
use rml_rtmp::time::RtmpTimestamp;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

/// Nom d'application RTMP attendu : `rtmp://127.0.0.1:<port>/live`.
pub const INGEST_APP: &str = "live";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestState {
    /// Serveur arrêté (ou pas encore démarré).
    #[default]
    Stopped,
    /// Serveur à l'écoute, aucun encodeur connecté.
    Waiting,
    /// Un encodeur publie un flux.
    Live,
    /// Le serveur n'a pas pu démarrer (ex. port occupé).
    Error,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct IngestStatus {
    pub state: IngestState,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f32>,
    pub bitrate_kbps: Option<u32>,
    pub video_codec: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone)]
enum Media {
    Video(u32, Bytes),
    Audio(u32, Bytes),
    Meta(StreamMetadata),
    /// L'encodeur s'est déconnecté : les lecteurs doivent se fermer.
    End,
}

#[derive(Default)]
struct Cache {
    metadata: Option<StreamMetadata>,
    video_header: Option<Bytes>,
    audio_header: Option<Bytes>,
}

struct Shared {
    key: String,
    media: broadcast::Sender<Media>,
    publishing: Mutex<bool>,
    cache: Mutex<Cache>,
    bytes: AtomicU64,
    frames: AtomicU64,
    status: watch::Sender<IngestStatus>,
}

impl Shared {
    fn begin_publish(&self) -> bool {
        let mut publishing = self.publishing.lock().unwrap();
        if *publishing {
            return false;
        }
        *publishing = true;
        *self.cache.lock().unwrap() = Cache::default();
        self.bytes.store(0, Ordering::Relaxed);
        self.frames.store(0, Ordering::Relaxed);
        self.status.send_modify(|s| {
            *s = IngestStatus {
                state: IngestState::Live,
                ..Default::default()
            }
        });
        true
    }

    fn end_publish(&self) {
        *self.publishing.lock().unwrap() = false;
        *self.cache.lock().unwrap() = Cache::default();
        let _ = self.media.send(Media::End);
        self.status.send_modify(|s| {
            *s = IngestStatus {
                state: IngestState::Waiting,
                ..Default::default()
            }
        });
    }

    fn on_video(&self, ts: u32, data: Bytes) {
        self.bytes.fetch_add(data.len() as u64, Ordering::Relaxed);
        let info = VideoInfo::parse(&data);
        if info.sequence_header {
            self.cache.lock().unwrap().video_header = Some(data.clone());
        }
        if info.is_frame {
            self.frames.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(codec) = info.codec {
            self.status.send_if_modified(|s| {
                let changed = s.video_codec.as_deref() != Some(codec);
                if changed {
                    s.video_codec = Some(codec.to_string());
                }
                changed
            });
        }
        let _ = self.media.send(Media::Video(ts, data));
    }

    fn on_audio(&self, ts: u32, data: Bytes) {
        self.bytes.fetch_add(data.len() as u64, Ordering::Relaxed);
        if is_audio_sequence_header(&data) {
            self.cache.lock().unwrap().audio_header = Some(data.clone());
        }
        let _ = self.media.send(Media::Audio(ts, data));
    }

    fn on_metadata(&self, metadata: StreamMetadata) {
        self.status.send_modify(|s| {
            s.width = metadata.video_width;
            s.height = metadata.video_height;
            if s.fps.is_none() {
                s.fps = metadata.video_frame_rate;
            }
        });
        self.cache.lock().unwrap().metadata = Some(metadata.clone());
        let _ = self.media.send(Media::Meta(metadata));
    }
}

struct VideoInfo {
    sequence_header: bool,
    is_frame: bool,
    codec: Option<&'static str>,
}

impl VideoInfo {
    fn parse(data: &[u8]) -> Self {
        let none = Self {
            sequence_header: false,
            is_frame: false,
            codec: None,
        };
        let Some(&b) = data.first() else { return none };
        if b & 0x80 != 0 {
            // Enhanced RTMP : le type de paquet est dans les 4 bits bas, puis le FourCC.
            let packet_type = b & 0x0f;
            let codec = match data.get(1..5) {
                Some(b"avc1") => Some("H.264"),
                Some(b"hvc1") => Some("H.265"),
                Some(b"av01") => Some("AV1"),
                Some(b"vp09") => Some("VP9"),
                _ => None,
            };
            return Self {
                sequence_header: packet_type == 0,
                is_frame: matches!(packet_type, 1 | 3),
                codec,
            };
        }
        let codec = match b & 0x0f {
            7 => Some("H.264"),
            12 => Some("H.265"),
            _ => None,
        };
        let frame_type = b >> 4;
        let avc_packet = data.get(1).copied();
        Self {
            sequence_header: avc_packet == Some(0),
            is_frame: frame_type != 5 && avc_packet == Some(1),
            codec,
        }
    }
}

fn is_audio_sequence_header(data: &[u8]) -> bool {
    match data.first() {
        // Enhanced RTMP (SoundFormat 9) : paquet « SequenceStart ».
        Some(b) if b >> 4 == 9 => b & 0x0f == 0,
        // AAC (SoundFormat 10) : AACPacketType 0.
        Some(b) if b >> 4 == 10 => data.get(1) == Some(&0),
        _ => false,
    }
}

/// Garde de publication : libère l'ingest quel que soit le motif de sortie.
struct PublishGuard(Arc<Shared>);

impl Drop for PublishGuard {
    fn drop(&mut self) {
        self.0.end_publish();
    }
}

pub struct Ingest {
    port: u16,
    key: String,
    status: watch::Receiver<IngestStatus>,
    tasks: Vec<JoinHandle<()>>,
}

impl Ingest {
    /// Démarre l'écoute sur `127.0.0.1:port` (0 = port libre choisi par l'OS).
    /// En cas d'échec (port occupé), l'erreur est aussi publiée dans le statut.
    pub async fn start(port: u16, key: impl Into<String>) -> Result<Self, String> {
        let key = key.into();
        let (status_tx, status_rx) = watch::channel(IngestStatus::default());
        let listener = match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(l) => l,
            Err(e) => {
                let msg = if e.kind() == std::io::ErrorKind::AddrInUse {
                    format!("Le port {port} est déjà utilisé (une autre instance d'Omnistream ou un autre serveur RTMP ?). Changez le port d'entrée dans les réglages.")
                } else {
                    format!("Impossible d'ouvrir le port {port} : {e}")
                };
                status_tx.send_modify(|s| {
                    s.state = IngestState::Error;
                    s.error = Some(msg.clone());
                });
                return Err(msg);
            }
        };
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
        status_tx.send_modify(|s| s.state = IngestState::Waiting);

        let (media, _) = broadcast::channel(1024);
        let shared = Arc::new(Shared {
            key: key.clone(),
            media,
            publishing: Mutex::new(false),
            cache: Mutex::default(),
            bytes: AtomicU64::new(0),
            frames: AtomicU64::new(0),
            status: status_tx,
        });

        let accept_shared = shared.clone();
        let accept = tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    continue;
                };
                let shared = accept_shared.clone();
                tokio::spawn(async move {
                    let _ = sock.set_nodelay(true);
                    let _ = handle_connection(sock, shared).await;
                });
            }
        });

        let stats_shared = shared;
        let stats = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            tick.tick().await;
            loop {
                tick.tick().await;
                if !*stats_shared.publishing.lock().unwrap() {
                    continue;
                }
                let bytes = stats_shared.bytes.swap(0, Ordering::Relaxed);
                let frames = stats_shared.frames.swap(0, Ordering::Relaxed);
                stats_shared.status.send_modify(|s| {
                    s.bitrate_kbps = Some((bytes * 8 / 1000) as u32);
                    if frames > 0 {
                        s.fps = Some(frames as f32);
                    }
                });
            }
        });

        Ok(Self {
            port,
            key,
            status: status_rx,
            tasks: vec![accept, stats],
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn url(&self) -> String {
        format!("rtmp://127.0.0.1:{}/{INGEST_APP}", self.port)
    }

    /// URL complète à donner à FFmpeg pour lire le flux.
    pub fn play_url(&self) -> String {
        format!("{}/{}", self.url(), self.key)
    }

    pub fn status(&self) -> watch::Receiver<IngestStatus> {
        self.status.clone()
    }
}

impl Drop for Ingest {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

enum Role {
    None,
    Publisher(#[allow(dead_code)] PublishGuard),
    Player { stream_id: u32 },
}

async fn handle_connection(mut sock: TcpStream, shared: Arc<Shared>) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    let mut buf = vec![0u8; 16 * 1024];

    let mut handshake = Handshake::new(PeerType::Server);
    let leftover = loop {
        let n = sock.read(&mut buf).await.map_err(io)?;
        if n == 0 {
            return Ok(());
        }
        match handshake
            .process_bytes(&buf[..n])
            .map_err(|e| format!("{e:?}"))?
        {
            HandshakeProcessResult::InProgress { response_bytes } => {
                sock.write_all(&response_bytes).await.map_err(io)?;
            }
            HandshakeProcessResult::Completed {
                response_bytes,
                remaining_bytes,
            } => {
                sock.write_all(&response_bytes).await.map_err(io)?;
                break remaining_bytes;
            }
        }
    };

    let (mut session, initial) =
        ServerSession::new(ServerSessionConfig::new()).map_err(|e| format!("{e:?}"))?;
    let mut role = Role::None;
    let mut media_rx: Option<broadcast::Receiver<Media>> = None;
    let mut queue: VecDeque<ServerSessionResult> = initial.into();
    if !leftover.is_empty() {
        queue.extend(
            session
                .handle_input(&leftover)
                .map_err(|e| format!("{e:?}"))?,
        );
    }

    loop {
        while let Some(result) = queue.pop_front() {
            match result {
                ServerSessionResult::OutboundResponse(p) => {
                    sock.write_all(&p.bytes).await.map_err(io)?
                }
                ServerSessionResult::RaisedEvent(event) => {
                    on_event(
                        event,
                        &shared,
                        &mut session,
                        &mut queue,
                        &mut role,
                        &mut media_rx,
                    )?;
                }
                _ => {}
            }
        }

        tokio::select! {
            n = sock.read(&mut buf) => {
                let n = n.map_err(io)?;
                if n == 0 {
                    return Ok(());
                }
                queue.extend(session.handle_input(&buf[..n]).map_err(|e| format!("{e:?}"))?);
            }
            media = async {
                match media_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                let Role::Player { stream_id } = role else { continue };
                let packet = match media {
                    Ok(Media::Video(ts, data)) => session.send_video_data(stream_id, data, RtmpTimestamp::new(ts), false),
                    Ok(Media::Audio(ts, data)) => session.send_audio_data(stream_id, data, RtmpTimestamp::new(ts), false),
                    Ok(Media::Meta(m)) => session.send_metadata(stream_id, &m),
                    // Fin de flux, ou lecteur trop lent : on ferme, FFmpeg sera relancé.
                    Ok(Media::End) | Err(_) => return Ok(()),
                };
                sock.write_all(&packet.map_err(|e| format!("{e:?}"))?.bytes).await.map_err(io)?;
            }
        }
    }
}

fn on_event(
    event: ServerSessionEvent,
    shared: &Arc<Shared>,
    session: &mut ServerSession,
    queue: &mut VecDeque<ServerSessionResult>,
    role: &mut Role,
    media_rx: &mut Option<broadcast::Receiver<Media>>,
) -> Result<(), String> {
    let e = |e: rml_rtmp::sessions::ServerSessionError| format!("{e:?}");
    match event {
        ServerSessionEvent::ConnectionRequested {
            request_id,
            app_name,
        } => {
            if app_name == INGEST_APP {
                queue.extend(session.accept_request(request_id).map_err(e)?);
            } else {
                queue.extend(
                    session
                        .reject_request(
                            request_id,
                            "NetConnection.Connect.Rejected",
                            "Application inconnue",
                        )
                        .map_err(e)?,
                );
            }
        }
        ServerSessionEvent::PublishStreamRequested {
            request_id,
            stream_key,
            ..
        } => {
            if stream_key == shared.key && shared.begin_publish() {
                *role = Role::Publisher(PublishGuard(shared.clone()));
                queue.extend(session.accept_request(request_id).map_err(e)?);
            } else {
                let why = if stream_key == shared.key {
                    "Un encodeur publie déjà"
                } else {
                    "Clé de stream invalide"
                };
                queue.extend(
                    session
                        .reject_request(request_id, "NetStream.Publish.BadName", why)
                        .map_err(e)?,
                );
            }
        }
        ServerSessionEvent::PlayStreamRequested {
            request_id,
            stream_key,
            stream_id,
            ..
        } => {
            if stream_key != shared.key {
                queue.extend(
                    session
                        .reject_request(request_id, "NetStream.Play.StreamNotFound", "Clé invalide")
                        .map_err(e)?,
                );
                return Ok(());
            }
            // On s'abonne avant de lire le cache pour ne perdre aucun paquet.
            *media_rx = Some(shared.media.subscribe());
            *role = Role::Player { stream_id };
            queue.extend(session.accept_request(request_id).map_err(e)?);
            let cache = shared.cache.lock().unwrap();
            let zero = RtmpTimestamp::new(0);
            let mut packets = Vec::new();
            if let Some(m) = &cache.metadata {
                packets.push(session.send_metadata(stream_id, m).map_err(e)?);
            }
            if let Some(h) = &cache.video_header {
                packets.push(
                    session
                        .send_video_data(stream_id, h.clone(), zero, false)
                        .map_err(e)?,
                );
            }
            if let Some(h) = &cache.audio_header {
                packets.push(
                    session
                        .send_audio_data(stream_id, h.clone(), zero, false)
                        .map_err(e)?,
                );
            }
            queue.extend(
                packets
                    .into_iter()
                    .map(ServerSessionResult::OutboundResponse),
            );
        }
        ServerSessionEvent::StreamMetadataChanged { metadata, .. }
            if matches!(role, Role::Publisher(_)) =>
        {
            shared.on_metadata(metadata);
        }
        ServerSessionEvent::VideoDataReceived {
            data, timestamp, ..
        } if matches!(role, Role::Publisher(_)) => {
            shared.on_video(timestamp.value, data);
        }
        ServerSessionEvent::AudioDataReceived {
            data, timestamp, ..
        } if matches!(role, Role::Publisher(_)) => {
            shared.on_audio(timestamp.value, data);
        }
        ServerSessionEvent::PublishStreamFinished { .. } => {
            *role = Role::None;
        }
        _ => {}
    }
    Ok(())
}
