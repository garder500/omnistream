//! OAuth Google/YouTube (flux « authorization code » + PKCE pour application de bureau).
//! L'utilisateur crée ses propres identifiants OAuth de type « Application de bureau » dans
//! Google Cloud ; Google redirige vers une adresse locale que l'application écoute le temps
//! de la connexion (`http://127.0.0.1:41258/callback`).

use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::loopback::{self, encode};
use super::{now_secs, TokenSet};
use crate::{Error, Result};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub(crate) const API_URL: &str = "https://www.googleapis.com/youtube/v3";

pub const CALLBACK_PORT: u16 = 41258;
pub const REDIRECT_URI: &str = "http://127.0.0.1:41258/callback";

/// Lecture/écriture du chat, modération et infos de live.
pub const SCOPE: &str = "https://www.googleapis.com/auth/youtube.force-ssl";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeUser {
    pub channel_id: String,
    pub title: String,
    /// Handle (`@nom`) sans l'arobase, s'il existe.
    pub handle: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

/// Vérificateur PKCE (43 caractères) et son empreinte S256.
pub fn pkce_pair(random: [u8; 32]) -> (String, String) {
    let verifier = URL_SAFE_NO_PAD.encode(random);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// Nouvelle paire PKCE issue de l'aléa du système.
pub fn new_pkce() -> Result<(String, String)> {
    let mut random = [0u8; 32];
    getrandom::getrandom(&mut random)
        .map_err(|e| Error::Other(format!("aléa système indisponible: {e}")))?;
    Ok(pkce_pair(random))
}

/// Adresse à ouvrir dans le navigateur pour autoriser l'application.
pub fn authorize_url(client_id: &str, state: &str, challenge: &str) -> String {
    format!(
        "{AUTH_URL}?client_id={}&response_type=code&scope={}&redirect_uri={}&state={}\
         &code_challenge={challenge}&code_challenge_method=S256&access_type=offline&prompt=consent",
        encode(client_id),
        encode(SCOPE),
        encode(REDIRECT_URI),
        encode(state)
    )
}

pub async fn wait_for_code(expected_state: &str, timeout: Duration) -> Result<String> {
    loopback::wait_for_code(CALLBACK_PORT, "YouTube", expected_state, timeout).await
}

fn parse_tokens(status: u16, body: &str) -> Result<TokenSet> {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    if status != 200 || v.get("access_token").is_none() {
        let msg = v["error_description"]
            .as_str()
            .or(v["error"].as_str())
            .unwrap_or(body);
        return Err(Error::Other(format!(
            "Google a refusé la demande de jeton ({status}): {msg}"
        )));
    }
    let t: TokenResponse = serde_json::from_value(v)
        .map_err(|e| Error::Other(format!("réponse Google invalide: {e}")))?;
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
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", REDIRECT_URI),
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
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ],
    )
    .await
    .map_err(|e| Error::Other(format!("renouvellement du jeton YouTube échoué: {e}")))
}

pub(crate) fn parse_user(v: &serde_json::Value) -> Result<YoutubeUser> {
    let item = &v["items"][0];
    let channel_id = item["id"]
        .as_str()
        .ok_or_else(|| Error::Other("aucune chaîne YouTube associée à ce compte Google".into()))?;
    Ok(YoutubeUser {
        channel_id: channel_id.to_string(),
        title: item["snippet"]["title"]
            .as_str()
            .unwrap_or(channel_id)
            .to_string(),
        handle: item["snippet"]["customUrl"]
            .as_str()
            .map(|h| h.trim_start_matches('@').to_lowercase())
            .filter(|h| !h.is_empty()),
    })
}

/// Chaîne YouTube du compte connecté.
pub async fn fetch_user(http: &reqwest::Client, access_token: &str) -> Result<YoutubeUser> {
    let resp = http
        .get(format!("{API_URL}/channels?part=snippet&mine=true"))
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
    fn pkce_matches_rfc7636_example() {
        // Exemple de la RFC 7636, annexe B (octets du vérificateur).
        let octets = [
            116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22, 212,
            37, 77, 105, 214, 191, 240, 91, 88, 5, 88, 83, 132, 141, 121,
        ];
        let (verifier, challenge) = pkce_pair(octets);
        assert_eq!(verifier, "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn builds_authorize_url() {
        let url = authorize_url("id.apps", "s 1", "chal");
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?client_id=id.apps"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A41258%2Fcallback"));
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fyoutube.force-ssl"));
        assert!(url.contains("code_challenge=chal&code_challenge_method=S256"));
        assert!(url.contains("access_type=offline"));
    }

    #[test]
    fn parses_token_responses() {
        let t = parse_tokens(
            200,
            r#"{"access_token":"a","refresh_token":"r","expires_in":3599,"token_type":"Bearer"}"#,
        )
        .unwrap();
        assert_eq!(t.refresh_token.as_deref(), Some("r"));
        assert!(t.expires_at.unwrap() > now_secs());
        let e = parse_tokens(
            400,
            r#"{"error":"invalid_grant","error_description":"Bad Request"}"#,
        );
        assert!(e.unwrap_err().to_string().contains("Bad Request"));
    }

    #[test]
    fn parses_user() {
        let u = parse_user(&serde_json::json!({"items":[{"id":"UC123","snippet":{"title":"Garder","customUrl":"@Garder500"}}]})).unwrap();
        assert_eq!(
            (u.channel_id.as_str(), u.handle.as_deref()),
            ("UC123", Some("garder500"))
        );
        assert!(parse_user(&serde_json::json!({"items":[]})).is_err());
    }
}
