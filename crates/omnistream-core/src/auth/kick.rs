//! OAuth Kick (API publique officielle : « authorization code » + PKCE obligatoire).
//! L'utilisateur crée son application sur kick.com (Paramètres > Développeur) et y déclare
//! l'adresse de redirection locale `http://127.0.0.1:41259/callback`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::loopback::{self, encode};
use super::{now_secs, TokenSet};
use crate::{Error, Result};

const AUTH_URL: &str = "https://id.kick.com/oauth/authorize";
const TOKEN_URL: &str = "https://id.kick.com/oauth/token";
pub(crate) const API_URL: &str = "https://api.kick.com/public/v1";

pub const CALLBACK_PORT: u16 = 41259;
pub const REDIRECT_URI: &str = "http://127.0.0.1:41259/callback";

pub const SCOPES: &[&str] = &[
    "user:read",
    "channel:read",
    "channel:write",
    "chat:write",
    "moderation:ban",
    "moderation:chat_message:manage",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KickUser {
    pub user_id: String,
    pub name: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

pub use super::pkce::{new_pkce, pkce_pair};

pub fn authorize_url(client_id: &str, state: &str, challenge: &str) -> String {
    format!(
        "{AUTH_URL}?client_id={}&response_type=code&scope={}&redirect_uri={}&state={}\
         &code_challenge={challenge}&code_challenge_method=S256",
        encode(client_id),
        SCOPES.join("%20"),
        encode(REDIRECT_URI),
        encode(state)
    )
}

pub async fn wait_for_code(expected_state: &str, timeout: Duration) -> Result<String> {
    loopback::wait_for_code(CALLBACK_PORT, "Kick", expected_state, timeout).await
}

fn parse_tokens(status: u16, body: &str) -> Result<TokenSet> {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    if status != 200 || v.get("access_token").is_none() {
        let msg = v["error_description"]
            .as_str()
            .or(v["error"].as_str())
            .or(v["message"].as_str())
            .unwrap_or(body);
        return Err(Error::Other(format!(
            "Kick a refusé la demande de jeton ({status}): {msg}"
        )));
    }
    let t: TokenResponse = serde_json::from_value(v)
        .map_err(|e| Error::Other(format!("réponse Kick invalide: {e}")))?;
    Ok(TokenSet {
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: t.expires_in.map(|s| now_secs() + s),
    })
}

async fn token_request(http: &reqwest::Client, form: &[(&str, &str)]) -> Result<TokenSet> {
    let resp = http.post(TOKEN_URL).form(form).send().await?;
    let status = resp.status().as_u16();
    let text = resp.text().await?;
    parse_tokens(status, &text)
}

pub async fn exchange_code(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    code: &str,
    verifier: &str,
) -> Result<TokenSet> {
    token_request(
        http,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", verifier),
            ("code", code),
        ],
    )
    .await
}

pub async fn refresh(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<TokenSet> {
    token_request(
        http,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
        ],
    )
    .await
    .map_err(|e| Error::Other(format!("renouvellement du jeton Kick échoué: {e}")))
}

pub(crate) fn parse_user(v: &serde_json::Value) -> Result<KickUser> {
    let item = &v["data"][0];
    let user_id = match &item["user_id"] {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) if !s.is_empty() => s.clone(),
        _ => return Err(Error::Other("profil Kick introuvable".into())),
    };
    Ok(KickUser {
        name: item["name"].as_str().unwrap_or(&user_id).to_string(),
        user_id,
    })
}

/// Profil du compte connecté (sans paramètre, `/users` renvoie le propriétaire du jeton).
pub async fn fetch_user(http: &reqwest::Client, access_token: &str) -> Result<KickUser> {
    let resp = http
        .get(format!("{API_URL}/users"))
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?;
    parse_user(&resp.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_authorize_url() {
        let url = authorize_url("cid", "s 1", "chal");
        assert!(url.starts_with("https://id.kick.com/oauth/authorize?client_id=cid"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A41259%2Fcallback"));
        assert!(url.contains("scope=user:read%20channel:read"));
        assert!(url.contains("code_challenge=chal&code_challenge_method=S256"));
    }

    #[test]
    fn parses_token_responses() {
        let t = parse_tokens(
            200,
            r#"{"access_token":"a","refresh_token":"r","expires_in":7200,"token_type":"Bearer"}"#,
        )
        .unwrap();
        assert_eq!(t.refresh_token.as_deref(), Some("r"));
        assert!(parse_tokens(400, r#"{"error":"invalid_grant"}"#)
            .unwrap_err()
            .to_string()
            .contains("invalid_grant"));
    }

    #[test]
    fn parses_user() {
        let u = parse_user(&serde_json::json!({"data":[{"user_id":123,"name":"Garder"}]})).unwrap();
        assert_eq!((u.user_id.as_str(), u.name.as_str()), ("123", "Garder"));
        assert!(parse_user(&serde_json::json!({"data":[]})).is_err());
    }
}
