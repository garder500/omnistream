//! OAuth Twitch via Device Code Flow (client « public » : aucun secret requis).
//! Prérequis : l'application Twitch doit être déclarée avec le type de client « Public ».

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{now_secs, TokenSet};
use crate::{Error, Result};

const DEVICE_URL: &str = "https://id.twitch.tv/oauth2/device";
const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const USERS_URL: &str = "https://api.twitch.tv/helix/users";

pub const SCOPES: &[&str] = &[
    "chat:read",
    "chat:edit",
    "moderator:manage:banned_users",
    "moderator:manage:chat_messages",
    "channel:manage:broadcast",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitchUser {
    pub id: String,
    pub login: String,
    pub display_name: String,
}

#[derive(Debug)]
pub(crate) enum PollOutcome {
    Tokens(TokenSet),
    Pending,
    SlowDown,
    Failed(String),
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

impl From<TokenResponse> for TokenSet {
    fn from(t: TokenResponse) -> Self {
        TokenSet {
            access_token: t.access_token,
            refresh_token: t.refresh_token,
            expires_at: t.expires_in.map(|s| now_secs() + s),
        }
    }
}

pub(crate) fn classify_poll(status: u16, body: &str) -> PollOutcome {
    if status == 200 {
        return match serde_json::from_str::<TokenResponse>(body) {
            Ok(t) => PollOutcome::Tokens(t.into()),
            Err(e) => PollOutcome::Failed(format!("réponse Twitch invalide: {e}")),
        };
    }
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| body.to_owned());
    match message.as_str() {
        "authorization_pending" => PollOutcome::Pending,
        "slow_down" => PollOutcome::SlowDown,
        _ => PollOutcome::Failed(message),
    }
}

pub async fn start_device_flow(http: &reqwest::Client, client_id: &str) -> Result<DeviceCode> {
    let resp = http
        .post(DEVICE_URL)
        .form(&[("client_id", client_id), ("scopes", &SCOPES.join(" "))])
        .send()
        .await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Other(format!(
            "Twitch a refusé la demande de connexion ({body}). Vérifiez le Client ID et que l'application est de type « Public »."
        )));
    }
    Ok(resp.json().await?)
}

/// Attend que l'utilisateur ait validé le code, puis renvoie les jetons.
pub async fn poll_device_flow(
    http: &reqwest::Client,
    client_id: &str,
    code: &DeviceCode,
) -> Result<TokenSet> {
    let mut interval = code.interval.max(1);
    let deadline = now_secs() + code.expires_in as i64;
    while now_secs() < deadline {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        let resp = http
            .post(TOKEN_URL)
            .form(&[
                ("client_id", client_id),
                ("scopes", &SCOPES.join(" ")),
                ("device_code", &code.device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await?;
        let status = resp.status().as_u16();
        let body = resp.text().await?;
        match classify_poll(status, &body) {
            PollOutcome::Tokens(t) => return Ok(t),
            PollOutcome::Pending => {}
            PollOutcome::SlowDown => interval += 5,
            PollOutcome::Failed(msg) => {
                return Err(Error::Other(format!("connexion Twitch échouée: {msg}")))
            }
        }
    }
    Err(Error::Other(
        "le code Twitch a expiré, relancez la connexion".into(),
    ))
}

pub async fn refresh(
    http: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> Result<TokenSet> {
    let resp = http
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await?;
    let status = resp.status().as_u16();
    let body = resp.text().await?;
    match classify_poll(status, &body) {
        PollOutcome::Tokens(t) => Ok(t),
        PollOutcome::Failed(msg) => Err(Error::Other(format!(
            "renouvellement du jeton Twitch échoué: {msg}"
        ))),
        _ => Err(Error::Other("renouvellement du jeton Twitch échoué".into())),
    }
}

pub async fn fetch_user(
    http: &reqwest::Client,
    client_id: &str,
    access_token: &str,
) -> Result<TwitchUser> {
    let resp = http
        .get(USERS_URL)
        .header("Client-Id", client_id)
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?;
    let v: serde_json::Value = resp.json().await?;
    serde_json::from_value(v["data"][0].clone())
        .map_err(|_| Error::Other("profil Twitch introuvable".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_device_flow_responses() {
        assert!(matches!(
            classify_poll(400, r#"{"status":400,"message":"authorization_pending"}"#),
            PollOutcome::Pending
        ));
        assert!(matches!(
            classify_poll(400, r#"{"message":"slow_down"}"#),
            PollOutcome::SlowDown
        ));
        assert!(
            matches!(classify_poll(400, r#"{"message":"access_denied"}"#), PollOutcome::Failed(m) if m == "access_denied")
        );
        match classify_poll(
            200,
            r#"{"access_token":"a","refresh_token":"r","expires_in":3600}"#,
        ) {
            PollOutcome::Tokens(t) => {
                assert_eq!(t.access_token, "a");
                assert_eq!(t.refresh_token.as_deref(), Some("r"));
                assert!(t.expires_at.unwrap() > now_secs());
            }
            other => panic!("attendu des jetons, reçu {other:?}"),
        }
    }
}
