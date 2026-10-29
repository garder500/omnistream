//! OAuth Trovo (flux « authorization code »). Trovo n'a pas de device flow :
//! l'utilisateur autorise dans son navigateur, Trovo redirige vers une adresse
//! locale (`http://127.0.0.1:41257/callback`) que l'application écoute le temps de la connexion.
//! L'application Trovo doit déclarer exactement cette adresse de redirection.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::{now_secs, TokenSet};
use crate::{Error, Result};

const AUTH_URL: &str = "https://open.trovo.live/page/login.html";
pub(crate) const API_URL: &str = "https://open-api.trovo.live/openplatform";

/// Port local fixe : à déclarer dans l'application Trovo.
pub const CALLBACK_PORT: u16 = 41257;
pub const REDIRECT_URI: &str = "http://127.0.0.1:41257/callback";

pub const SCOPES: &[&str] = &[
    "chat_connect",
    "chat_send_self",
    "send_to_my_channel",
    "manage_messages",
    "user_details_self",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrovoUser {
    pub user_id: String,
    pub login: String,
    pub display_name: String,
    pub channel_id: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let Some(b) = s
                    .get(i + 1..i + 3)
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                {
                    out.push(b);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Adresse à ouvrir dans le navigateur pour autoriser l'application.
pub fn authorize_url(client_id: &str, state: &str) -> String {
    format!(
        "{AUTH_URL}?client_id={}&response_type=code&scope={}&redirect_uri={}&state={}",
        encode(client_id),
        SCOPES.join("+"),
        encode(REDIRECT_URI),
        encode(state)
    )
}

/// Extrait le `code` de la première ligne d'une requête HTTP de redirection,
/// en vérifiant le paramètre `state` (protection CSRF).
pub(crate) fn parse_callback(request_line: &str, expected_state: &str) -> Result<String> {
    let target = request_line.split_whitespace().nth(1).unwrap_or_default();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/callback" {
        return Err(Error::Other("requête inattendue".into()));
    }
    let param = |name: &str| {
        query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == name)
            .map(|(_, v)| decode(v))
    };
    if let Some(e) = param("error") {
        return Err(Error::Other(format!("autorisation Trovo refusée: {e}")));
    }
    if param("state").as_deref() != Some(expected_state) {
        return Err(Error::Other(
            "réponse Trovo non reconnue (state invalide)".into(),
        ));
    }
    param("code")
        .filter(|c| !c.is_empty())
        .ok_or_else(|| Error::Other("Trovo n'a pas renvoyé de code d'autorisation".into()))
}

/// Écoute la redirection de Trovo et renvoie le code d'autorisation.
pub async fn wait_for_code(expected_state: &str, timeout: Duration) -> Result<String> {
    let listener = TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|e| {
            Error::Other(format!(
                "port {CALLBACK_PORT} indisponible pour la connexion Trovo: {e}"
            ))
        })?;
    let wait = async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = vec![0u8; 4096];
            // Les navigateurs ouvrent parfois des connexions muettes : on ne s'y attarde pas.
            let n = match tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf)).await {
                Ok(r) => r.unwrap_or(0),
                Err(_) => 0,
            };
            let head = String::from_utf8_lossy(&buf[..n]).into_owned();
            let line = head.lines().next().unwrap_or_default();
            if !line.contains("/callback") {
                // Requête parasite (favicon...) : on attend la vraie redirection.
                let _ = sock.shutdown().await;
                continue;
            }
            let result = parse_callback(line, expected_state);
            let (status, body) = match &result {
                Ok(_) => (
                    "200 OK",
                    "Omnistream est connecté à Trovo. Vous pouvez fermer cet onglet.",
                ),
                Err(_) => (
                    "400 Bad Request",
                    "Connexion Trovo impossible. Retournez dans Omnistream.",
                ),
            };
            let page = format!(
                "<!doctype html><meta charset=utf-8><title>Omnistream</title><body style=\"font-family:sans-serif;background:#120826;color:#fff;text-align:center;padding-top:20vh\">{body}"
            );
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                page.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.shutdown().await;
            // Une requête sans code ni erreur (autre page locale) n'annule pas la connexion.
            if result.is_ok() || line.contains("error=") {
                return Ok::<_, std::io::Error>(result);
            }
        }
    };
    match tokio::time::timeout(timeout, wait).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => Err(Error::Io(e)),
        Err(_) => Err(Error::Other(
            "délai dépassé: autorisation Trovo non reçue".into(),
        )),
    }
}

fn parse_tokens(status: u16, body: &str) -> Result<TokenSet> {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    if status != 200 || v.get("access_token").is_none() {
        let msg = v["message"]
            .as_str()
            .or(v["error"].as_str())
            .unwrap_or(body);
        return Err(Error::Other(format!(
            "Trovo a refusé la demande de jeton ({status}): {msg}"
        )));
    }
    let t: TokenResponse = serde_json::from_value(v)
        .map_err(|e| Error::Other(format!("réponse Trovo invalide: {e}")))?;
    Ok(TokenSet {
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: t.expires_in.map(|s| now_secs() + s),
    })
}

async fn token_request(
    http: &reqwest::Client,
    client_id: &str,
    path: &str,
    body: serde_json::Value,
) -> Result<TokenSet> {
    let resp = http
        .post(format!("{API_URL}/{path}"))
        .header("client-id", client_id)
        .json(&body)
        .send()
        .await?;
    let status = resp.status().as_u16();
    let text = resp.text().await?;
    parse_tokens(status, &text)
}

pub async fn exchange_code(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    code: &str,
) -> Result<TokenSet> {
    token_request(
        http,
        client_id,
        "exchangetoken",
        serde_json::json!({
            "client_secret": client_secret,
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": REDIRECT_URI,
        }),
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
        client_id,
        "refreshtoken",
        serde_json::json!({
            "client_secret": client_secret,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }),
    )
    .await
    .map_err(|e| Error::Other(format!("renouvellement du jeton Trovo échoué: {e}")))
}

pub(crate) fn parse_user(v: &serde_json::Value) -> Result<TrovoUser> {
    let text = |k: &str| match &v[k] {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    };
    let login = text("userName").ok_or_else(|| Error::Other("profil Trovo introuvable".into()))?;
    Ok(TrovoUser {
        user_id: text("userId").ok_or_else(|| Error::Other("profil Trovo introuvable".into()))?,
        display_name: text("nickName").unwrap_or_else(|| login.clone()),
        login: login.to_lowercase(),
        channel_id: text("channelId").unwrap_or_default(),
    })
}

pub async fn fetch_user(
    http: &reqwest::Client,
    client_id: &str,
    access_token: &str,
) -> Result<TrovoUser> {
    let resp = http
        .get(format!("{API_URL}/getuserinfo"))
        .header("client-id", client_id)
        .header("Authorization", format!("OAuth {access_token}"))
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
        let url = authorize_url("abc", "s 1");
        assert!(url.starts_with(
            "https://open.trovo.live/page/login.html?client_id=abc&response_type=code"
        ));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A41257%2Fcallback"));
        assert!(url.contains("state=s%201"));
        assert!(url.contains("scope=chat_connect+chat_send_self"));
    }

    #[test]
    fn parses_callback() {
        assert_eq!(
            parse_callback("GET /callback?code=a%2Fb&state=xyz HTTP/1.1", "xyz").unwrap(),
            "a/b"
        );
        assert!(parse_callback("GET /callback?code=a&state=bad HTTP/1.1", "xyz").is_err());
        assert!(parse_callback(
            "GET /callback?error=access_denied&state=xyz HTTP/1.1",
            "xyz"
        )
        .is_err());
        assert!(parse_callback("GET /favicon.ico HTTP/1.1", "xyz").is_err());
        assert!(parse_callback("GET /callback?state=xyz HTTP/1.1", "xyz").is_err());
    }

    #[test]
    fn parses_token_responses() {
        let t = parse_tokens(
            200,
            r#"{"access_token":"a","refresh_token":"r","expires_in":3600,"scope":"x"}"#,
        )
        .unwrap();
        assert_eq!(t.access_token, "a");
        assert_eq!(t.refresh_token.as_deref(), Some("r"));
        assert!(t.expires_at.unwrap() > now_secs());
        assert!(parse_tokens(400, r#"{"message":"invalid code"}"#).is_err());
    }

    #[test]
    fn parses_user() {
        let u = parse_user(&serde_json::json!({"userId":"123","userName":"Garder","nickName":"Garder500","channelId":"456"})).unwrap();
        assert_eq!(
            (u.user_id.as_str(), u.login.as_str(), u.channel_id.as_str()),
            ("123", "garder", "456")
        );
        assert_eq!(u.display_name, "Garder500");
        assert!(parse_user(&serde_json::json!({})).is_err());
    }
}
