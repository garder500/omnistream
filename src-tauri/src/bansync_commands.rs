//! Commandes IPC des bans synchronisés (fine couche sur `omnistream_core::bansync`).

use omnistream_core::bansync::{IdentityLink, Job, PreviewItem, SyncRequest};
use omnistream_core::domain::Platform;
use tauri::State;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: omnistream_core::Error) -> String {
    e.to_string()
}

#[tauri::command]
pub fn bansync_list_identities(state: State<'_, AppState>) -> CmdResult<Vec<IdentityLink>> {
    state.bansync.list_identities().map_err(err)
}

#[tauri::command]
pub fn bansync_link_identity(state: State<'_, AppState>, link: IdentityLink) -> CmdResult<()> {
    state.bansync.link_identity(&link).map_err(err)
}

#[tauri::command]
pub fn bansync_unlink_identity(state: State<'_, AppState>, platform: Platform, user_id: String) -> CmdResult<()> {
    state.bansync.unlink_identity(&platform, &user_id).map_err(err)
}

#[tauri::command]
pub fn bansync_preview(state: State<'_, AppState>, request: SyncRequest) -> CmdResult<Vec<PreviewItem>> {
    state.bansync.preview(&request).map_err(err)
}

/// Met en file puis traite la demande ; les échecs restent consultables dans le journal.
#[tauri::command]
pub async fn bansync_submit(state: State<'_, AppState>, request: SyncRequest) -> CmdResult<Vec<Job>> {
    let job = state.bansync.enqueue(&request).map_err(err)?;
    state.bansync.process(Some(&job)).await.map_err(err)?;
    state.bansync.journal(30).map_err(err)
}

#[tauri::command]
pub async fn bansync_retry(state: State<'_, AppState>, job_id: Option<String>) -> CmdResult<Vec<Job>> {
    state.bansync.retry_failed(job_id.as_deref()).await.map_err(err)?;
    state.bansync.journal(30).map_err(err)
}

#[tauri::command]
pub fn bansync_journal(state: State<'_, AppState>) -> CmdResult<Vec<Job>> {
    state.bansync.journal(30).map_err(err)
}
