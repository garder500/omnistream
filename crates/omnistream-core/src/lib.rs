//! Cœur d'Omnistream : tout ce qui ne dépend pas de Tauri.
//!
//! Le crate `src-tauri` n'est qu'une fine couche de commandes IPC au-dessus.

pub mod auth;
pub mod automod;
pub mod connectors;
pub mod db;
pub mod domain;
pub mod emotes;
pub mod error;
pub mod ffmpeg;
pub mod ingest;
pub mod moderation;
pub mod secrets;
pub mod streaming;

pub use error::{Error, Result};
