//! Commandes Tauri propres à TikTok (lecture seule, via le service de signature Euler Stream).

use serde::Serialize;
use tauri::State;

use crate::state::AppState;

const API_KEY_SETTING: &str = "tiktok_euler_api_key";

fn api_key(state: &AppState) -> Option<String> {
    state
        .db
        .get_setting(API_KEY_SETTING)
        .ok()
        .flatten()
        .filter(|k| !k.is_empty())
}

/// Recharge la clé enregistrée (appelé au démarrage).
pub async fn restore(state: &AppState) {
    state.tiktok.set_api_key(api_key(state)).await;
}

#[derive(Serialize)]
pub struct TiktokSettings {
    has_key: bool,
}

#[tauri::command]
pub fn get_tiktok_settings(state: State<'_, AppState>) -> TiktokSettings {
    TiktokSettings {
        has_key: api_key(&state).is_some(),
    }
}

/// Enregistre la clé d'API Euler Stream ; une clé vide la supprime. Les chaînes TikTok se reconnectent.
#[tauri::command]
pub async fn set_tiktok_api_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    let key = key.trim().to_string();
    state
        .db
        .set_setting(API_KEY_SETTING, &key)
        .map_err(|e| e.to_string())?;
    state
        .tiktok
        .set_api_key(Some(key).filter(|k| !k.is_empty()))
        .await;
    crate::commands::reconnect_channels(&state).await;
    Ok(())
}
