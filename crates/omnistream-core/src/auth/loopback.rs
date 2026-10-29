//! Redirection locale (`http://127.0.0.1:<port>/callback`) des flux OAuth « authorization code »
//! de bureau : l'application écoute le temps de la connexion et récupère le `code`.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::{Error, Result};

pub(crate) fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub(crate) fn decode(s: &str) -> String {
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

/// Extrait le `code` de la première ligne d'une requête HTTP de redirection,
/// en vérifiant le paramètre `state` (protection CSRF).
pub(crate) fn parse_callback(
    request_line: &str,
    expected_state: &str,
    provider: &str,
) -> Result<String> {
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
        return Err(Error::Other(format!(
            "autorisation {provider} refusée: {e}"
        )));
    }
    if param("state").as_deref() != Some(expected_state) {
        return Err(Error::Other(format!(
            "réponse {provider} non reconnue (state invalide)"
        )));
    }
    param("code")
        .filter(|c| !c.is_empty())
        .ok_or_else(|| Error::Other(format!("{provider} n'a pas renvoyé de code d'autorisation")))
}

/// Écoute la redirection de Trovo et renvoie le code d'autorisation.
pub async fn wait_for_code(
    port: u16,
    provider: &str,
    expected_state: &str,
    timeout: Duration,
) -> Result<String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await.map_err(|e| {
        Error::Other(format!(
            "port {port} indisponible pour la connexion {provider}: {e}"
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
            let result = parse_callback(line, expected_state, provider);
            let (status, body) = match &result {
                Ok(_) => (
                    "200 OK",
                    format!("Omnistream est connecté à {provider}. Vous pouvez fermer cet onglet."),
                ),
                Err(_) => (
                    "400 Bad Request",
                    format!("Connexion {provider} impossible. Retournez dans Omnistream."),
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
        Err(_) => Err(Error::Other(format!(
            "délai dépassé: autorisation {provider} non reçue"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_callback() {
        let parse = |line| parse_callback(line, "xyz", "Test");
        assert_eq!(
            parse("GET /callback?code=a%2Fb&state=xyz HTTP/1.1").unwrap(),
            "a/b"
        );
        assert!(parse("GET /callback?code=a&state=bad HTTP/1.1").is_err());
        assert!(parse("GET /callback?error=access_denied&state=xyz HTTP/1.1").is_err());
        assert!(parse("GET /favicon.ico HTTP/1.1").is_err());
        assert!(parse("GET /callback?state=xyz HTTP/1.1").is_err());
    }

    #[test]
    fn encodes_and_decodes() {
        assert_eq!(encode("a b/é"), "a%20b%2F%C3%A9");
        assert_eq!(decode("a%20b+c%2F"), "a b c/");
    }
}
