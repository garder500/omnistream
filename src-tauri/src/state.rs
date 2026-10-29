use std::path::PathBuf;
use std::sync::Arc;

use omnistream_core::automation::SystemClock;
use omnistream_core::automation_service::AutomationService;
use omnistream_core::automod::AutoMod;
use omnistream_core::bansync::BanSync;
use omnistream_core::connectors::{
    ChatEvent, ConnectorRegistry, KickConnector, TiktokConnector, TrovoConnector, TwitchConnector,
    YoutubeConnector,
};
use omnistream_core::db::Database;
use omnistream_core::moderation::ModerationEngine;
use omnistream_core::overlay::OverlayService;
use omnistream_core::secrets::KeyringStore;
use omnistream_core::streaming::StreamService;
use tokio::sync::mpsc;

pub struct AppState {
    pub db: Arc<Database>,
    pub twitch: TwitchConnector,
    pub trovo: TrovoConnector,
    pub youtube: YoutubeConnector,
    pub kick: KickConnector,
    pub tiktok: TiktokConnector,
    pub connectors: ConnectorRegistry,
    pub moderation: ModerationEngine,
    pub automation: Arc<AutomationService>,
    pub automod: std::sync::Mutex<AutoMod>,
    pub bansync: BanSync,
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

        let trovo = TrovoConnector::new();
        let trovo_db = db.clone();
        trovo.on_token_refresh(Arc::new(move |user_id, tokens| {
            if let Err(e) = trovo_db.save_tokens(user_id, tokens) {
                eprintln!("impossible d'enregistrer les jetons Trovo renouvelés: {e}");
            }
        }));

        let youtube = YoutubeConnector::new();
        let youtube_db = db.clone();
        youtube.on_token_refresh(Arc::new(move |user_id, tokens| {
            if let Err(e) = youtube_db.save_tokens(user_id, tokens) {
                eprintln!("impossible d'enregistrer les jetons YouTube renouvelés: {e}");
            }
        }));

        let kick = KickConnector::new();
        let kick_db = db.clone();
        kick.on_token_refresh(Arc::new(move |user_id, tokens| {
            if let Err(e) = kick_db.save_tokens(user_id, tokens) {
                eprintln!("impossible d'enregistrer les jetons Kick renouvelés: {e}");
            }
        }));

        let tiktok = TiktokConnector::new();

        let mut connectors = ConnectorRegistry::new();
        connectors.register(Arc::new(twitch.clone()));
        connectors.register(Arc::new(trovo.clone()));
        connectors.register(Arc::new(youtube.clone()));
        connectors.register(Arc::new(kick.clone()));
        connectors.register(Arc::new(tiktok.clone()));
        let (chat_tx, chat_rx) = mpsc::channel(1024);
        let streams = StreamService::new(db.clone(), Arc::new(KeyringStore));
        let automation = Arc::new(AutomationService::new(
            db.clone(),
            connectors.clone(),
            Arc::new(SystemClock),
        )?);
        let automod =
            std::sync::Mutex::new(AutoMod::new(db.list_automod_rules().unwrap_or_else(|e| {
                eprintln!("automod: règles illisibles, automod désactivé: {e}");
                Vec::new()
            })));
        let state = Self {
            db: db.clone(),
            automod,
            automation,
            twitch,
            trovo,
            youtube,
            kick,
            tiktok,
            moderation: ModerationEngine::new(connectors.clone()),
            bansync: BanSync::new(db.clone(), connectors.clone()),
            connectors,
            streams,
            overlay,
            chat_tx,
        };
        Ok((state, chat_rx))
    }
}
