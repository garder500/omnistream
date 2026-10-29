//! Démo du serveur d'overlay, sans Tauri : `cargo run -p omnistream-core --example overlay_demo [port]`.
//! Ouvre l'URL affichée dans un navigateur : des messages de test arrivent après 3 secondes.

use std::time::Duration;

use omnistream_core::overlay::{start_server, Emote, OverlayAuthor, OverlayHub, OverlayMessage};

fn message(id: &str, platform: &str, name: &str, text: &str, emotes: Vec<Emote>) -> OverlayMessage {
    OverlayMessage {
        id: id.into(),
        platform: platform.into(),
        author: OverlayAuthor {
            display_name: name.into(),
            is_moderator: id == "2",
            is_subscriber: false,
        },
        text: text.into(),
        timestamp_ms: 0,
        emotes,
        highlighted: false,
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let port = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    let hub = OverlayHub::new();
    let server = start_server(hub.clone(), "demo-token".into(), port)
        .await
        .expect("démarrage");
    println!(
        "http://127.0.0.1:{}/overlay/chat?token=demo-token",
        server.port()
    );

    tokio::time::sleep(Duration::from_secs(3)).await;
    let smile = Emote {
        code: "Kappa".into(),
        url: "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='28' height='28'%3E%3Ccircle cx='14' cy='14' r='13' fill='%23fbbf24'/%3E%3C/svg%3E".into(),
    };
    hub.chat_message(message(
        "1",
        "twitch",
        "Ana",
        "Bienvenue sur le stream Kappa",
        vec![smile],
    ));
    hub.chat_message(message(
        "2",
        "youtube",
        "Bob",
        "Salut tout le monde <b>pas de HTML</b>",
        vec![],
    ));
    hub.chat_message(message(
        "3",
        "kick",
        "Chloé",
        "Super contenu aujourd'hui",
        vec![],
    ));
    hub.highlight("3", true);
    hub.pin(message(
        "4",
        "twitch",
        "Dan",
        "Règle du jour : soyez sympas !",
        vec![],
    ));
    tokio::time::sleep(Duration::from_secs(600)).await;
}
