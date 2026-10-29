//! Cœur d'Omnistream : tout ce qui ne dépend pas de Tauri.
//!
//! Le crate `src-tauri` n'est qu'une fine couche de commandes IPC au-dessus.

pub mod auth;
pub mod connectors;
pub mod db;
pub mod domain;
pub mod error;
pub mod moderation;
pub mod streaming;

pub use error::{Error, Result};
