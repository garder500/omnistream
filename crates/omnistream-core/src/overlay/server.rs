//! Serveur HTTP/WebSocket local des overlays (OBS « source navigateur »).
//!
//! Garde-fous : écoute sur `127.0.0.1` uniquement, jeton obligatoire dans l'URL
//! (`?token=...`) et en-tête `Host` limité à la boucle locale (anti DNS-rebinding).

use std::net::{Ipv4Addr, SocketAddr};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::hub::OverlayHub;
use crate::{Error, Result};

const CHAT_HTML: &str = include_str!("assets/chat.html");

#[derive(Clone)]
struct ServerState {
    hub: OverlayHub,
    token: String,
    /// Se ferme quand le serveur s'arrête : les WebSocket ouverts s'arrêtent avec lui.
    shutdown: watch::Receiver<()>,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

/// Serveur en cours d'exécution ; l'arrêter (ou le lâcher) ferme aussi les connexions.
pub struct RunningServer {
    port: u16,
    task: Option<JoinHandle<()>>,
    _shutdown: watch::Sender<()>,
}

impl RunningServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Arrête le serveur, coupe les connexions ouvertes et attend que le port soit libéré.
    pub async fn stop(mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// Démarre le serveur sur `127.0.0.1:port` (`0` = port libre choisi par le système).
pub async fn start(hub: OverlayHub, token: String, port: u16) -> Result<RunningServer> {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .await
        .map_err(|e| {
            Error::Other(format!(
                "impossible d'ouvrir le port {port} pour l'overlay: {e}"
            ))
        })?;
    let port = listener.local_addr()?.port();
    let (shutdown_tx, shutdown) = watch::channel(());
    let app = router(ServerState {
        hub,
        token,
        shutdown,
    });
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(RunningServer {
        port,
        task: Some(task),
        _shutdown: shutdown_tx,
    })
}

fn router(state: ServerState) -> Router {
    Router::new()
        .route("/overlay/chat", get(chat_page))
        .route("/overlay/chat/theme.css", get(chat_theme))
        .route("/ws/:channel", get(ws_handler))
        .layer(middleware::from_fn(local_host_only))
        .with_state(state)
}

/// Compare sans court-circuit pour ne pas révéler le jeton par le temps de réponse.
fn token_matches(expected: &str, given: Option<&str>) -> bool {
    let Some(given) = given else { return false };
    let (a, b) = (expected.as_bytes(), given.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

fn authorized(state: &ServerState, q: &TokenQuery) -> bool {
    token_matches(&state.token, q.token.as_deref())
}

async fn local_host_only(req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    // Une page web tierce ne doit pas pouvoir lire le chat : on refuse toute origine non locale.
    let origin = req.headers().get(header::ORIGIN).map(|o| {
        o.to_str()
            .unwrap_or("")
            .split_once("://")
            .map_or("", |(_, rest)| rest)
    });
    if is_local_host(host) && origin.is_none_or(is_local_host) {
        next.run(req).await
    } else {
        StatusCode::FORBIDDEN.into_response()
    }
}

/// `host[:port]` désigne-t-il la boucle locale ?
fn is_local_host(host_port: &str) -> bool {
    let name = host_port.strip_prefix('[').map_or_else(
        || host_port.split(':').next().unwrap_or(""),
        |rest| rest.split(']').next().unwrap_or(""),
    );
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

fn no_store(mut res: Response) -> Response {
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

async fn chat_page(State(s): State<ServerState>, Query(q): Query<TokenQuery>) -> Response {
    if !authorized(&s, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    no_store(Html(CHAT_HTML).into_response())
}

async fn chat_theme(State(s): State<ServerState>, Query(q): Query<TokenQuery>) -> Response {
    if !authorized(&s, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    no_store(
        (
            [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
            s.hub.settings().css(),
        )
            .into_response(),
    )
}

fn valid_channel(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

async fn ws_handler(
    State(s): State<ServerState>,
    Path(channel): Path<String>,
    Query(q): Query<TokenQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    if !authorized(&s, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !valid_channel(&channel) {
        return StatusCode::NOT_FOUND.into_response();
    }
    ws.on_upgrade(move |socket| serve_socket(socket, s.hub, channel, s.shutdown))
}

async fn serve_socket(
    mut socket: WebSocket,
    hub: OverlayHub,
    channel: String,
    mut shutdown: watch::Receiver<()>,
) {
    // Abonnement avant l'état initial : aucun événement ne peut se perdre entre les deux.
    let mut rx = hub.subscribe(&channel);
    if socket
        .send(Message::Text(hub.snapshot(&channel)))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            // L'émetteur est lâché à l'arrêt du serveur : `changed` renvoie alors une erreur.
            _ = shutdown.changed() => break,
            event = rx.recv() => match event {
                Ok(payload) => {
                    if socket.send(Message::Text(payload)).await.is_err() {
                        break;
                    }
                }
                // Page trop lente : on lui renvoie l'état complet.
                Err(RecvError::Lagged(_)) => {
                    if socket.send(Message::Text(hub.snapshot(&channel))).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_comparison() {
        assert!(token_matches("abc", Some("abc")));
        assert!(!token_matches("abc", Some("abd")));
        assert!(!token_matches("abc", Some("abcd")));
        assert!(!token_matches("abc", None));
    }

    #[test]
    fn local_hosts() {
        assert!(
            is_local_host("127.0.0.1:17420")
                && is_local_host("localhost")
                && is_local_host("[::1]:80")
        );
        assert!(
            !is_local_host("evil.example")
                && !is_local_host("127.0.0.1.evil.example")
                && !is_local_host("")
        );
    }

    #[test]
    fn channel_names() {
        assert!(valid_channel("chat") && valid_channel("alerts-2"));
        assert!(!valid_channel("") && !valid_channel("../x") && !valid_channel("A"));
    }
}
