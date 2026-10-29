//! Serveur d'overlays local pour OBS : voir [`OverlayService`].

mod hub;
mod server;
mod service;

pub use hub::{
    Emote, OverlayAuthor, OverlayHub, OverlayMessage, OverlaySettings, ALERTS_CHANNEL, CHAT_CHANNEL,
};
pub use server::{start as start_server, RunningServer};
pub use service::{OverlayInfo, OverlayService, DEFAULT_PORT};
