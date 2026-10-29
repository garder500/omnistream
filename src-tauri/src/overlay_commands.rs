//! Commandes IPC du serveur d'overlay (réglages, épinglage, surlignage).

use omnistream_core::domain::ChatMessage;
use omnistream_core::overlay::{OverlayAuthor, OverlayInfo, OverlayMessage, OverlaySettings};
use tauri::State;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: omnistream_core::Error) -> String {
    e.to_string()
}

#[tauri::command]
pub async fn overlay_info(state: State<'_, AppState>) -> CmdResult<OverlayInfo> {
    state.overlay.info().await.map_err(err)
}

#[tauri::command]
pub async fn overlay_set_port(state: State<'_, AppState>, port: u16) -> CmdResult<OverlayInfo> {
    state.overlay.set_port(port).await.map_err(err)?;
    state.overlay.info().await.map_err(err)
}

#[tauri::command]
pub async fn overlay_regenerate_token(state: State<'_, AppState>) -> CmdResult<OverlayInfo> {
    state.overlay.regenerate_token().await.map_err(err)?;
    state.overlay.info().await.map_err(err)
}

#[tauri::command]
pub fn overlay_get_settings(state: State<'_, AppState>) -> OverlaySettings {
    state.overlay.hub().settings()
}

#[tauri::command]
pub fn overlay_set_settings(
    state: State<'_, AppState>,
    settings: OverlaySettings,
) -> CmdResult<OverlaySettings> {
    state.overlay.set_settings(settings).map_err(err)
}

#[tauri::command]
pub fn overlay_pin(state: State<'_, AppState>, message: ChatMessage) {
    state.overlay.hub().pin(OverlayMessage::from(&message));
}

#[tauri::command]
pub fn overlay_unpin(state: State<'_, AppState>) {
    state.overlay.hub().unpin();
}

#[tauri::command]
pub fn overlay_highlight(state: State<'_, AppState>, message_id: String, on: bool) {
    state.overlay.hub().highlight(&message_id, on);
}

#[tauri::command]
pub fn overlay_clear(state: State<'_, AppState>) {
    state.overlay.hub().clear();
}

/// Message factice pour vérifier le rendu dans OBS sans attendre le chat.
#[tauri::command]
pub fn overlay_test_message(state: State<'_, AppState>) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    state.overlay.hub().chat_message(OverlayMessage {
        id: format!("test-{now}"),
        platform: "twitch".into(),
        author: OverlayAuthor {
            display_name: "Omnistream".into(),
            is_moderator: true,
            is_subscriber: false,
        },
        text: "Ceci est un message de test pour votre overlay".into(),
        timestamp_ms: now,
        emotes: Vec::new(),
        highlighted: false,
    });
}
