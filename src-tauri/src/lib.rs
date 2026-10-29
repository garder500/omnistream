mod commands;
mod state;

use omnistream_core::connectors::ChatEvent;
use omnistream_core::moderation::flag_reason;
use serde_json::json;
use tauri::{Emitter, Manager};

use state::AppState;

/// Relaie les événements des connecteurs vers l'interface.
async fn forward_chat_events(app: tauri::AppHandle, mut rx: tokio::sync::mpsc::Receiver<ChatEvent>) {
    while let Some(event) = rx.recv().await {
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
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
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
            commands::start_destination,
            commands::stop_destination,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement d'Omnistream");
}
