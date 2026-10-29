mod commands;
mod state;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(AppState::new(dir.join("omnistream.db"))?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::list_destinations,
            commands::save_destination,
            commands::start_destination,
            commands::stop_destination,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement d'Omnistream");
}
