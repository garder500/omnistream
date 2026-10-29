use omnistream_core::domain::{Account, StreamDestination};
use tauri::State;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: omnistream_core::Error) -> String {
    e.to_string()
}

#[tauri::command]
pub fn list_accounts(state: State<'_, AppState>) -> CmdResult<Vec<Account>> {
    state.db.list_accounts().map_err(err)
}

#[tauri::command]
pub fn list_destinations(state: State<'_, AppState>) -> CmdResult<Vec<StreamDestination>> {
    state.db.list_destinations().map_err(err)
}

#[tauri::command]
pub fn save_destination(state: State<'_, AppState>, destination: StreamDestination) -> CmdResult<()> {
    state.db.save_destination(&destination).map_err(err)
}

#[tauri::command]
pub async fn start_destination(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let dest = state
        .db
        .list_destinations()
        .map_err(err)?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("destination inconnue: {id}"))?;
    state.streams.lock().await.start(&dest).map_err(err)
}

#[tauri::command]
pub async fn stop_destination(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.streams.lock().await.stop(&id).await.map_err(err)
}
