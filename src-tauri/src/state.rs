use std::path::PathBuf;
use std::sync::Arc;

use omnistream_core::connectors::{ChatEvent, ConnectorRegistry, TwitchConnector};
use omnistream_core::db::Database;
use omnistream_core::moderation::ModerationEngine;
use omnistream_core::overlay::OverlayService;
use omnistream_core::secrets::KeyringStore;
use omnistream_core::streaming::StreamService;
use tokio::sync::mpsc;

pub struct AppState {
    pub db: Arc<Database>,
    pub twitch: TwitchConnector,
    pub connectors: ConnectorRegistry,
    pub moderation: ModerationEngine,
    pub streams: StreamService,
    pub overlay: OverlayService,
    pub chat_tx: mpsc::Sender<ChatEvent>,
}

impl AppState {
    /// Retourne aussi le récepteur des événements de chat, à relayer vers l'interface.
    pub fn new(db_path: PathBuf) -> omnistream_core::Result<(Self, mpsc::Receiver<ChatEvent>)> {
        let db = Arc::new(Database::open(db_path)?);
        let overlay = OverlayService::new(db.clone());
        let twitch = TwitchConnector::new();
        let hook_db = db.clone();
        twitch.on_token_refresh(Arc::new(move |user_id, tokens| {
            if let Err(e) = hook_db.save_tokens(user_id, tokens) {
                eprintln!("impossible d'enregistrer les jetons renouvelés: {e}");
            }
        }));

        let mut connectors = ConnectorRegistry::new();
        connectors.register(Arc::new(twitch.clone()));
        let (chat_tx, chat_rx) = mpsc::channel(1024);
        let streams = StreamService::new(db.clone(), Arc::new(KeyringStore));
        let state = Self {
            db,
            twitch,
            moderation: ModerationEngine::new(connectors.clone()),
            connectors,
            streams,
            overlay,
            chat_tx,
        };
        Ok((state, chat_rx))
    }
}
