//! Cycle de vie du serveur d'overlay : port et jeton persistés en base, réglages du thème.

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

use super::hub::{OverlayHub, OverlaySettings};
use super::server::{self, RunningServer};
use crate::db::Database;
use crate::{Error, Result};

pub const DEFAULT_PORT: u16 = 17420;

const PORT_KEY: &str = "overlay_port";
const TOKEN_KEY: &str = "overlay_token";
const SETTINGS_KEY: &str = "overlay_settings";

/// Ce que l'interface affiche : URL à copier dans OBS et état du serveur.
#[derive(Debug, Clone, Serialize)]
pub struct OverlayInfo {
    pub running: bool,
    pub port: u16,
    pub token: String,
    pub chat_url: String,
    /// Dernière erreur de démarrage (port occupé...), le cas échéant.
    pub error: Option<String>,
}

pub struct OverlayService {
    db: Arc<Database>,
    hub: OverlayHub,
    server: Mutex<Option<RunningServer>>,
    last_error: std::sync::Mutex<Option<String>>,
}

fn new_token() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|e| Error::Other(format!("génération du jeton impossible: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

impl OverlayService {
    pub fn new(db: Arc<Database>) -> Self {
        let hub = OverlayHub::new();
        if let Ok(Some(raw)) = db.get_setting(SETTINGS_KEY) {
            if let Ok(settings) = serde_json::from_str::<OverlaySettings>(&raw) {
                hub.set_settings(settings);
            }
        }
        Self { db, hub, server: Mutex::new(None), last_error: std::sync::Mutex::new(None) }
    }

    /// Hub de diffusion (clonable) : c'est lui que les autres modules utilisent pour publier.
    pub fn hub(&self) -> &OverlayHub {
        &self.hub
    }

    fn port(&self) -> u16 {
        self.db
            .get_setting(PORT_KEY)
            .ok()
            .flatten()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT)
    }

    fn token(&self) -> Result<String> {
        if let Some(t) = self.db.get_setting(TOKEN_KEY)?.filter(|t| t.len() >= 16) {
            return Ok(t);
        }
        let t = new_token()?;
        self.db.set_setting(TOKEN_KEY, &t)?;
        Ok(t)
    }

    /// Démarre le serveur (sans effet s'il tourne déjà). Une erreur est aussi mémorisée pour [`Self::info`].
    pub async fn start(&self) -> Result<()> {
        let mut guard = self.server.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let result = async { server::start(self.hub.clone(), self.token()?, self.port()).await }.await;
        match result {
            Ok(s) => {
                *self.last_error.lock().unwrap() = None;
                *guard = Some(s);
                Ok(())
            }
            Err(e) => {
                *self.last_error.lock().unwrap() = Some(e.to_string());
                Err(e)
            }
        }
    }

    pub async fn stop(&self) {
        if let Some(s) = self.server.lock().await.take() {
            s.stop().await;
        }
    }

    pub async fn restart(&self) -> Result<()> {
        self.stop().await;
        self.start().await
    }

    pub async fn set_port(&self, port: u16) -> Result<()> {
        if port < 1024 {
            return Err(Error::Other("choisissez un port supérieur à 1023".into()));
        }
        let mut guard = self.server.lock().await;
        if guard.as_ref().is_some_and(|s| s.port() == port) {
            return Ok(());
        }
        // On ouvre le nouveau port avant de lâcher l'ancien : en cas d'échec rien ne change.
        let new = server::start(self.hub.clone(), self.token()?, port).await;
        let new = match new {
            Ok(s) => s,
            Err(e) => {
                *self.last_error.lock().unwrap() = Some(e.to_string());
                return Err(e);
            }
        };
        self.db.set_setting(PORT_KEY, &port.to_string())?;
        *self.last_error.lock().unwrap() = None;
        if let Some(old) = guard.replace(new) {
            old.stop().await;
        }
        Ok(())
    }

    /// Invalide l'ancienne URL (les sources OBS existantes doivent être mises à jour).
    pub async fn regenerate_token(&self) -> Result<()> {
        self.db.set_setting(TOKEN_KEY, &new_token()?)?;
        self.restart().await
    }

    pub fn set_settings(&self, settings: OverlaySettings) -> Result<OverlaySettings> {
        let settings = self.hub.set_settings(settings);
        let raw = serde_json::to_string(&settings).map_err(|e| Error::Other(e.to_string()))?;
        self.db.set_setting(SETTINGS_KEY, &raw)?;
        Ok(settings)
    }

    pub async fn info(&self) -> Result<OverlayInfo> {
        let running = self.server.lock().await.as_ref().map(RunningServer::port);
        let port = running.unwrap_or_else(|| self.port());
        let token = self.token()?;
        Ok(OverlayInfo {
            running: running.is_some(),
            port,
            chat_url: format!("http://127.0.0.1:{port}/overlay/chat?token={token}"),
            token,
            error: self.last_error.lock().unwrap().clone(),
        })
    }
}
