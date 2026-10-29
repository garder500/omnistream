mod commands;
mod state;
mod trovo;

use omnistream_core::connectors::ChatEvent;
use omnistream_core::emotes::EmoteCatalog;
use serde_json::json;
use tauri::{Emitter, Manager};

use state::AppState;

/// Relaie les événements des connecteurs vers l'interface.
async fn forward_chat_events(
    app: tauri::AppHandle,
    mut rx: tokio::sync::mpsc::Receiver<ChatEvent>,
    emotes: EmoteCatalog,
) {
    // Automatisations (commandes !cmd, règles) : un seul worker, donc événements traités dans l'ordre,
    // et hors de la boucle pour ne jamais bloquer l'affichage du chat. File pleine = événement ignoré.
    let automation = app.state::<AppState>().automation.clone();
    let (auto_tx, mut auto_rx) = tokio::sync::mpsc::channel::<ChatEvent>(1024);
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = auto_rx.recv().await {
            automation.on_chat_event(&ev).await;
        }
    });
    while let Some(event) = rx.recv().await {
        let _ = auto_tx.try_send(event.clone());
        let _ = match event {
            ChatEvent::Message(mut m) => {
                emotes.annotate(&mut m);
                let verdict = app.state::<AppState>().automod.lock().unwrap().evaluate(&m);
                if !verdict.moderation_kinds(&m).is_empty() {
                    let (handle, msg, v) = (app.clone(), m.clone(), verdict.clone());
                    tauri::async_runtime::spawn(async move {
                        commands::apply_automod(&handle, &msg, &v).await
                    });
                }
                app.emit(
                    "chat://message",
                    json!({ "message": m, "flag": verdict.reason(), "score": verdict.score }),
                )
            }
            ChatEvent::Event(e) => {
                if let Err(err) = app
                    .state::<AppState>()
                    .db
                    .record_event(&e, commands::now_ms())
                {
                    eprintln!("historique des événements: {err}");
                }
                app.emit("chat://event", e)
            }
            ChatEvent::MessageDeleted {
                channel_id,
                message_id,
            } => app.emit(
                "chat://deleted",
                json!({ "channel_id": channel_id, "message_id": message_id }),
            ),
            ChatEvent::Connected { channel_id } => app.emit(
                "chat://status",
                json!({ "channel_id": channel_id, "connected": true }),
            ),
            ChatEvent::Disconnected { channel_id } => app.emit(
                "chat://status",
                json!({ "channel_id": channel_id, "connected": false }),
            ),
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

/// Minuteurs (chaque seconde) et détection début/fin de live (chaque minute).
fn spawn_automation_loops(app: tauri::AppHandle) {
    let automation = app.state::<AppState>().automation.clone();
    let ticker = automation.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            ticker.tick().await;
        }
    });
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            automation.poll_live().await;
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    builder
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let (state, chat_rx) = AppState::new(dir.join("omnistream.db"))?;
            app.manage(state);

            let emotes = EmoteCatalog::new(dir.join("emotes"));
            tauri::async_runtime::spawn(forward_chat_events(app.handle().clone(), chat_rx, emotes));
            tauri::async_runtime::spawn(forward_streaming_events(app.handle().clone()));
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Ouvre l'ingest RTMP local ; une erreur (port occupé) est visible dans l'interface.
                let _ = handle.state::<AppState>().streams.start_ingest().await;
                commands::restore_session(&handle.state::<AppState>()).await;
            });
            spawn_automation_loops(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_twitch_client_id,
            commands::list_accounts,
            commands::twitch_login_start,
            commands::remove_account,
            trovo::get_trovo_settings,
            trovo::set_trovo_app,
            trovo::trovo_login_start,
            commands::list_channels,
            commands::add_channel,
            commands::remove_channel,
            commands::send_message,
            commands::moderate,
            commands::list_automod_rules,
            commands::save_automod_rule,
            commands::delete_automod_rule,
            commands::test_automod,
            commands::stream_infos,
            commands::list_sessions,
            commands::session_detail,
            commands::save_session_csv,
            commands::delete_session,
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
            commands::list_rules,
            commands::save_rule,
            commands::delete_rule,
            commands::list_chat_commands,
            commands::save_chat_command,
            commands::delete_chat_command,
            commands::automation_log,
            commands::update_stream_info,
            commands::search_categories,
            commands::list_presets,
            commands::save_preset,
            commands::delete_preset,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement d'Omnistream");
}
