//! Fournisseurs d'authentification OAuth, un par plateforme.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::domain::Platform;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Expiration en secondes epoch Unix, si connue.
    pub expires_at: Option<i64>,
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    fn platform(&self) -> Platform;

    /// URL vers laquelle envoyer l'utilisateur pour autoriser Omnistream.
    fn authorize_url(&self, state: &str, redirect_uri: &str) -> String;

    /// Échange le code reçu sur le redirect contre des tokens.
    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<TokenSet>;

    async fn refresh(&self, refresh_token: &str) -> Result<TokenSet>;
}

/// Squelette Twitch : seule l'URL d'autorisation est construite pour l'instant.
pub struct TwitchAuth {
    pub client_id: String,
}

#[async_trait]
impl AuthProvider for TwitchAuth {
    fn platform(&self) -> Platform {
        Platform::Twitch
    }

    fn authorize_url(&self, state: &str, redirect_uri: &str) -> String {
        format!(
            "https://id.twitch.tv/oauth2/authorize?client_id={}&redirect_uri={}&response_type=code&scope=chat:read+chat:edit+moderator:manage:banned_users&state={}",
            self.client_id, redirect_uri, state
        )
    }

    async fn exchange_code(&self, _code: &str, _redirect_uri: &str) -> Result<TokenSet> {
        Err(Error::NotImplemented("TwitchAuth::exchange_code"))
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<TokenSet> {
        Err(Error::NotImplemented("TwitchAuth::refresh"))
    }
}
