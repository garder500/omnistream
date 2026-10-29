mod commands;
mod overlay_commands;
mod state;

use omnistream_core::connectors::ChatEvent;
use omnistream_core::moderation::flag_reason;
use serde_json::json;
use tauri::{Emitter, Manager};

use state::AppState;

/// Relaie les événements des connecteurs vers l'interface.
async fn forward_chat_events(app: tauri::AppHandle, mut rx: tokio::sync::mpsc::Receiver<ChatEvent>) {
    while let Some(event) = rx.recv().await {
        // Même flux pour l'overlay OBS et pour l'interface.
        let hub = app.state::<AppState>().overlay.hub().clone();
        match &event {
            ChatEvent::Message(m) => hub.chat_domain_message(m),
            ChatEvent::MessageDeleted { message_id, .. } => hub.delete(message_id),
            _ => {}
        }
        let _ = match event {
            ChatEvent::Message(m) => {
                let flag = flag_reason(&m.text);
                app.emit("chat://message", json!({ "message": m, "flag": flag }))
            }
            ChatEvent::Event(e) => app.emit("chat://event", e),
            ChatEvent::MessageDeleted { channel_id, message_id } => {
                app.emit("chat://deleted", json!({ "channel_id": channel_id, "message_id": message_id }))
            }
            ChatEvent::Connected { channel_id } => {
                app.emit("chat://status", json!({ "channel_id": channel_id, "connected": true }))
            }
            ChatEvent::Disconnected { channel_id } => {
                app.emit("chat://status", json!({ "channel_id": channel_id, "connected": false }))
            }
        };
    }
}

/// Relaie l'état de l'ingest et des destinations vers l'interface.
async fn forward_streaming_events(app: tauri::AppHandle) {
    let mut rx = app.state::<AppState>().streams.subscribe();
    loop {
        match rx.recv().await {
            Ok(event) => {
                let _ = app.emit("stream://event", event);
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => break,
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let (state, chat_rx) = AppState::new(dir.join("omnistream.db"))?;
            app.manage(state);

            tauri::async_runtime::spawn(forward_chat_events(app.handle().clone(), chat_rx));
            tauri::async_runtime::spawn(forward_streaming_events(app.handle().clone()));
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Un port occupé n'empêche pas l'appli de démarrer : l'erreur s'affiche dans « Overlays ».
                let _ = handle.state::<AppState>().overlay.start().await;
                // Ouvre l'ingest RTMP local ; une erreur (port occupé) est visible dans l'interface.
                let _ = handle.state::<AppState>().streams.start_ingest().await;
                commands::restore_session(&handle.state::<AppState>()).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_twitch_client_id,
            commands::list_accounts,
            commands::twitch_login_start,
            commands::remove_account,
            commands::list_channels,
            commands::add_channel,
            commands::remove_channel,
            commands::send_message,
            commands::moderate,
            commands::stream_infos,
            commands::list_destinations,
            commands::save_destination,
            commands::streaming_state,
            commands::delete_destination,
            commands::go_live,
            commands::stop_live,
            commands::set_ingest_port,
            commands::set_ffmpeg_path,
            commands::set_stream_title,
            commands::twitch_current_title,
            overlay_commands::overlay_info,
            overlay_commands::overlay_set_port,
            overlay_commands::overlay_regenerate_token,
            overlay_commands::overlay_get_settings,
            overlay_commands::overlay_set_settings,
            overlay_commands::overlay_pin,
            overlay_commands::overlay_unpin,
            overlay_commands::overlay_highlight,
            overlay_commands::overlay_clear,
            overlay_commands::overlay_test_message,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement d'Omnistream");
}
