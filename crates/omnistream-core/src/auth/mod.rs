//! Authentification OAuth par plateforme.
//!
//! Twitch utilise le « Device Code Flow » : pas de secret client à embarquer
//! dans l'application ni de redirection locale, l'utilisateur saisit un code
//! sur twitch.tv/activate.

pub mod twitch;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Expiration en secondes epoch Unix, si connue.
    pub expires_at: Option<i64>,
}

pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
