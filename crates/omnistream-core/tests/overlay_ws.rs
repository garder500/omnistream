use std::time::Duration;

use futures_util::StreamExt;
use omnistream_core::overlay::{start_server, OverlayAuthor, OverlayHub, OverlayMessage, ALERTS_CHANNEL};
use serde_json::{json, Value};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const TOKEN: &str = "jeton-de-test-0123456789";

fn msg(id: &str, text: &str) -> OverlayMessage {
    OverlayMessage {
        id: id.into(),
        platform: "twitch".into(),
        author: OverlayAuthor { display_name: "Ana".into(), is_moderator: false, is_subscriber: true },
        text: text.into(),
        timestamp_ms: 1,
        emotes: vec![],
        highlighted: false,
    }
}

async fn next_json<S>(ws: &mut S) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let m = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("délai dépassé")
            .expect("flux fermé")
            .expect("erreur WS");
        if let Message::Text(t) = m {
            return serde_json::from_str(&t).unwrap();
        }
    }
}

#[tokio::test]
async fn websocket_receives_snapshot_then_live_events() {
    let hub = OverlayHub::new();
    hub.chat_message(msg("old", "avant"));
    let server = start_server(hub.clone(), TOKEN.into(), 0).await.unwrap();
    let port = server.port();

    let (mut ws, _) = connect_async(format!("ws://127.0.0.1:{port}/ws/chat?token={TOKEN}")).await.unwrap();
    let snap = next_json(&mut ws).await;
    assert_eq!(snap["type"], "snapshot");
    assert_eq!(snap["data"]["messages"][0]["id"], "old");

    hub.chat_message(msg("m1", "salut"));
    let live = next_json(&mut ws).await;
    assert_eq!((live["channel"].as_str(), live["type"].as_str()), (Some("chat"), Some("message")));
    assert_eq!(live["data"]["text"], "salut");

    hub.pin(msg("m1", "salut"));
    assert_eq!(next_json(&mut ws).await["type"], "pin");
    hub.highlight("m1", true);
    let hl = next_json(&mut ws).await;
    assert_eq!((hl["type"].as_str(), hl["data"]["on"].as_bool()), (Some("highlight"), Some(true)));

    // Canal générique « alerts » : même mécanique, sans page dédiée.
    let (mut alerts, _) = connect_async(format!("ws://127.0.0.1:{port}/ws/alerts?token={TOKEN}")).await.unwrap();
    assert_eq!(next_json(&mut alerts).await["type"], "hello");
    hub.publish(ALERTS_CHANNEL, "sub", &json!({ "who": "Bob" }));
    let a = next_json(&mut alerts).await;
    assert_eq!((a["channel"].as_str(), a["data"]["who"].as_str()), (Some("alerts"), Some("Bob")));

    server.stop().await;
}

#[tokio::test]
async fn wrong_or_missing_token_is_rejected() {
    let server = start_server(OverlayHub::new(), TOKEN.into(), 0).await.unwrap();
    let port = server.port();

    assert!(connect_async(format!("ws://127.0.0.1:{port}/ws/chat?token=mauvais")).await.is_err());
    assert!(connect_async(format!("ws://127.0.0.1:{port}/ws/chat")).await.is_err());

    let http = reqwest::Client::new();
    let status = |url: String| {
        let http = http.clone();
        async move { http.get(url).send().await.unwrap().status().as_u16() }
    };
    assert_eq!(status(format!("http://127.0.0.1:{port}/overlay/chat")).await, 401);
    assert_eq!(status(format!("http://127.0.0.1:{port}/overlay/chat?token={TOKEN}")).await, 200);
    assert_eq!(status(format!("http://127.0.0.1:{port}/overlay/chat/theme.css?token={TOKEN}")).await, 200);

    // Hôte étranger (DNS rebinding) refusé même avec le bon jeton.
    let res = http
        .get(format!("http://127.0.0.1:{port}/overlay/chat?token={TOKEN}"))
        .header("Host", "evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 403);

    server.stop().await;
}

#[tokio::test]
async fn page_and_theme_are_served() {
    let hub = OverlayHub::new();
    let server = start_server(hub.clone(), TOKEN.into(), 0).await.unwrap();
    let port = server.port();
    let page = reqwest::get(format!("http://127.0.0.1:{port}/overlay/chat?token={TOKEN}"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("/ws/chat"));

    let mut settings = hub.settings();
    settings.custom_css = ".msg { color: red; }".into();
    hub.set_settings(settings);
    let css = reqwest::get(format!("http://127.0.0.1:{port}/overlay/chat/theme.css?token={TOKEN}"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(css.contains(".msg { color: red; }"));
    server.stop().await;
}

#[tokio::test]
async fn stopping_the_server_closes_open_sockets_and_foreign_origins_are_refused() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let server = start_server(OverlayHub::new(), TOKEN.into(), 0).await.unwrap();
    let port = server.port();

    let mut req = format!("ws://127.0.0.1:{port}/ws/chat?token={TOKEN}").into_client_request().unwrap();
    req.headers_mut().insert("Origin", "https://evil.example".parse().unwrap());
    assert!(connect_async(req).await.is_err());

    let (mut ws, _) = connect_async(format!("ws://127.0.0.1:{port}/ws/chat?token={TOKEN}")).await.unwrap();
    assert_eq!(next_json(&mut ws).await["type"], "snapshot");
    server.stop().await;
    let end = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(Ok(m)) = ws.next().await {
            if matches!(m, Message::Close(_)) {
                break;
            }
        }
    })
    .await;
    assert!(end.is_ok(), "la connexion doit se fermer à l'arrêt du serveur");
}
