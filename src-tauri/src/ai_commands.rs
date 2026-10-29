//! Commandes IPC de l'assistant IA (clé API Claude fournie par l'utilisateur).
//!
//! La clé n'est jamais renvoyée à l'interface : seul `has_key` l'est.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use omnistream_core::ai::{AiChatLine, AiClient, AiConfig, AiVerdict, ReqwestTransport};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: omnistream_core::Error) -> String {
    e.to_string()
}

fn client() -> &'static AiClient<ReqwestTransport> {
    static CLIENT: OnceLock<AiClient<ReqwestTransport>> = OnceLock::new();
    CLIENT.get_or_init(AiClient::new)
}

#[derive(Serialize)]
pub struct AiSettings {
    enabled: bool,
    has_key: bool,
    moderation_model: String,
    summary_model: String,
}

fn view(cfg: AiConfig) -> AiSettings {
    AiSettings {
        enabled: cfg.enabled,
        has_key: cfg.api_key.is_some(),
        moderation_model: cfg.moderation_model,
        summary_model: cfg.summary_model,
    }
}

#[tauri::command]
pub fn ai_get_settings(state: State<'_, AppState>) -> CmdResult<AiSettings> {
    AiConfig::load(&state.db).map(view).map_err(err)
}

#[derive(Deserialize)]
pub struct AiSettingsInput {
    enabled: bool,
    /// Consentement explicite (confidentialité) : exigé pour passer de inactif à actif.
    #[serde(default)]
    consent: bool,
    /// `None` = conserver la clé actuelle ; `Some("")` = la supprimer.
    api_key: Option<String>,
    moderation_model: String,
    summary_model: String,
}

#[tauri::command]
pub fn ai_save_settings(state: State<'_, AppState>, settings: AiSettingsInput) -> CmdResult<AiSettings> {
    if let Some(key) = &settings.api_key {
        AiConfig::save_api_key(&state.db, key).map_err(err)?;
    }
    AiConfig::save_models(&state.db, &settings.moderation_model, &settings.summary_model).map_err(err)?;
    let has_key = AiConfig::load(&state.db).map_err(err)?.api_key.is_some();
    let was_enabled = AiConfig::load(&state.db).map_err(err)?.enabled;
    if settings.enabled && !was_enabled && !settings.consent {
        return Err("Le consentement à l'envoi des messages à Anthropic est requis pour activer l'IA.".into());
    }
    // Jamais actif sans clé.
    AiConfig::save_enabled(&state.db, settings.enabled && has_key).map_err(err)?;
    AiConfig::load(&state.db).map(view).map_err(err)
}

/// Note un message ; `None` si l'IA est inactive ou le message sans intérêt.
#[tauri::command]
pub async fn ai_score_message(state: State<'_, AppState>, text: String) -> CmdResult<Option<AiVerdict>> {
    let cfg = AiConfig::load(&state.db).map_err(err)?;
    client().score_message(&cfg, &text).await.map_err(err)
}

#[derive(Deserialize)]
pub struct AiSummaryRequest {
    /// "live" (arrivants tardifs) ou "recap" (bilan de fin de live).
    kind: String,
    lines: Vec<AiChatLine>,
    #[serde(default)]
    events: Vec<String>,
}

#[tauri::command]
pub async fn ai_summarize(state: State<'_, AppState>, request: AiSummaryRequest) -> CmdResult<String> {
    let cfg = AiConfig::load(&state.db).map_err(err)?;
    let text = match request.kind.as_str() {
        "live" => client().summarize_live(&cfg, &request.lines).await,
        "recap" => client().summarize_recap(&cfg, &request.lines, &request.events).await,
        other => return Err(format!("type de résumé inconnu: {other}")),
    }
    .map_err(err)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    let _ = state.db.save_ai_summary(&request.kind, &cfg.summary_model, &text, now);
    Ok(text)
}
