use std::path::PathBuf;
use std::sync::Arc;

use omnistream_core::connectors::{ConnectorRegistry, TwitchConnector};
use omnistream_core::db::Database;
use omnistream_core::moderation::ModerationEngine;
use omnistream_core::streaming::{StreamManager, DEFAULT_INGEST_URL};
use tokio::sync::Mutex;

pub struct AppState {
    pub db: Database,
    pub streams: Mutex<StreamManager>,
    #[allow(dead_code)] // branché sur l'UI de modération dans une prochaine étape
    pub moderation: ModerationEngine,
}

impl AppState {
    pub fn new(db_path: PathBuf) -> omnistream_core::Result<Self> {
        let mut connectors = ConnectorRegistry::new();
        connectors.register(Arc::new(TwitchConnector));
        Ok(Self {
            db: Database::open(db_path)?,
            streams: Mutex::new(StreamManager::new("ffmpeg", DEFAULT_INGEST_URL)),
            moderation: ModerationEngine::new(connectors),
        })
    }
}
