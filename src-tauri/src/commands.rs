use omnistream_core::auth::twitch::{self as twitch_auth, DeviceCode};
use omnistream_core::automation::{ChatCommand, Rule as AutomationRule};
use omnistream_core::automation_service::LogEntry;
use omnistream_core::automod::{validate_rule, Rule, Verdict};
use omnistream_core::connectors::TwitchCreds;
use omnistream_core::db::{SessionDetail, SessionSummary, ViewerSampleInput};
use omnistream_core::domain::{
    Account, Category, Channel, ChatAuthor, ChatMessage, ModerationAction, ModerationKind,
    Platform, PlatformUpdateResult, StreamInfo, StreamInfoUpdate, StreamPreset,
};
use omnistream_core::ffmpeg::FfmpegStatus;
use omnistream_core::streaming::{DestinationInput, DestinationView, StreamingState};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

const TWITCH_CLIENT_ID_KEY: &str = "twitch_client_id";

fn err(e: omnistream_core::Error) -> String {
    e.to_string()
}

fn twitch_client_id(state: &AppState) -> Option<String> {
    state
        .db
        .get_setting(TWITCH_CLIENT_ID_KEY)
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
        .or_else(|| option_env!("OMNISTREAM_TWITCH_CLIENT_ID").map(str::to_owned))
}

/// Recharge le compte Twitch enregistré (s'il y en a un) puis reconnecte les chaînes suivies.
pub async fn restore_session(state: &AppState) {
    if let (Some(client_id), Ok(accounts)) = (twitch_client_id(state), state.db.list_accounts()) {
        if let Some(acc) = accounts
            .into_iter()
            .find(|a| a.platform == Platform::Twitch)
        {
            if let Ok(Some(tokens)) = state.db.load_tokens(&acc.id) {
                state
                    .twitch
                    .set_credentials(Some(TwitchCreds {
                        client_id,
                        user_id: acc.id,
                        login: acc.login,
                        tokens,
                    }))
                    .await;
            }
        }
    }
    crate::trovo::restore(state).await;
    crate::youtube::restore(state).await;
    crate::kick::restore(state).await;
    reconnect_channels(state).await;
}

pub(crate) async fn reconnect_channels(state: &AppState) {
    for channel in state.db.list_channels().unwrap_or_default() {
        if let Ok(connector) = state.connectors.get(&channel.platform) {
            let _ = connector.connect(&channel, state.chat_tx.clone()).await;
        }
    }
}

#[derive(Serialize)]
pub struct Settings {
    twitch_client_id: Option<String>,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    Settings {
        twitch_client_id: twitch_client_id(&state),
    }
}

#[tauri::command]
pub fn set_twitch_client_id(state: State<'_, AppState>, client_id: String) -> CmdResult<()> {
    state
        .db
        .set_setting(TWITCH_CLIENT_ID_KEY, client_id.trim())
        .map_err(err)
}

#[tauri::command]
pub fn list_accounts(state: State<'_, AppState>) -> CmdResult<Vec<Account>> {
    state.db.list_accounts().map_err(err)
}

/// Démarre la connexion Twitch : renvoie le code à saisir sur twitch.tv/activate,
/// puis attend la validation en arrière-plan (événements `account://changed` / `account://error`).
#[tauri::command]
pub async fn twitch_login_start(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<DeviceCode> {
    let client_id = twitch_client_id(&state)
        .ok_or("Renseignez d'abord le Client ID de votre application Twitch (type « Public »).")?;
    let http = state.twitch.http().clone();
    let code = twitch_auth::start_device_flow(&http, &client_id)
        .await
        .map_err(err)?;

    let pending = code.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let outcome: omnistream_core::Result<Account> = async {
            let tokens = twitch_auth::poll_device_flow(&http, &client_id, &pending).await?;
            let user = twitch_auth::fetch_user(&http, &client_id, &tokens.access_token).await?;
            let account = Account {
                id: user.id.clone(),
                platform: Platform::Twitch,
                display_name: user.display_name,
                login: user.login.clone(),
            };
            state.db.save_account(&account)?;
            state.db.save_tokens(&account.id, &tokens)?;
            state
                .twitch
                .set_credentials(Some(TwitchCreds {
                    client_id,
                    user_id: user.id,
                    login: user.login,
                    tokens,
                }))
                .await;
            Ok(account)
        }
        .await;
        match outcome {
            Ok(account) => {
                // Reconnecte les chats pour qu'ils passent de l'anonyme au compte connecté.
                reconnect_channels(&state).await;
                let _ = app.emit("account://changed", account);
            }
            Err(e) => {
                let _ = app.emit("account://error", e.to_string());
            }
        }
    });
    Ok(code)
}

#[tauri::command]
pub async fn remove_account(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    let removed = state
        .db
        .list_accounts()
        .map_err(err)?
        .into_iter()
        .find(|a| a.id == id);
    state.db.delete_account(&id).map_err(err)?;
    match removed.map(|a| a.platform) {
        Some(Platform::Trovo) => state.trovo.set_account(None).await,
        Some(Platform::Youtube) => state.youtube.set_account(None).await,
        Some(Platform::Kick) => state.kick.set_account(None).await,
        _ => state.twitch.set_credentials(None).await,
    }
    reconnect_channels(&state).await;
    let _ = app.emit("account://changed", ());
    Ok(())
}

#[tauri::command]
pub fn list_channels(state: State<'_, AppState>) -> CmdResult<Vec<Channel>> {
    state.db.list_channels().map_err(err)
}

#[tauri::command]
pub async fn add_channel(
    state: State<'_, AppState>,
    platform: String,
    name: String,
) -> CmdResult<Channel> {
    let platform = Platform::from(platform.as_str());
    if !matches!(
        platform,
        Platform::Twitch | Platform::Trovo | Platform::Youtube | Platform::Kick
    ) {
        return Err(format!("{platform} n'est pas encore pris en charge."));
    }
    let trimmed = name.trim().trim_start_matches(['#', '@']);
    // Un identifiant de chaîne YouTube (UC...) est sensible à la casse ; handles et logins ne le sont pas.
    let login = if platform == Platform::Youtube && trimmed.starts_with("UC") {
        trimmed.to_string()
    } else {
        trimmed.to_lowercase()
    };
    let allowed = |c: char| {
        c.is_ascii_alphanumeric()
            || c == '_'
            || (platform == Platform::Youtube && matches!(c, '-' | '.'))
            || (platform == Platform::Kick && c == '-')
    };
    if login.is_empty() || !login.chars().all(allowed) {
        return Err("Nom de chaîne invalide.".into());
    }
    let channel = Channel {
        id: format!("{platform}:{login}"),
        platform,
        external_id: login.clone(),
        display_name: login,
    };
    state.db.save_channel(&channel).map_err(err)?;
    let connector = state.connectors.get(&channel.platform).map_err(err)?;
    connector
        .connect(&channel, state.chat_tx.clone())
        .await
        .map_err(err)?;
    let _ = state.automation.reload();
    Ok(channel)
}

fn find_channel(state: &AppState, id: &str) -> CmdResult<Channel> {
    state
        .db
        .list_channels()
        .map_err(err)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("chaîne inconnue: {id}"))
}

#[tauri::command]
pub async fn remove_channel(state: State<'_, AppState>, channel_id: String) -> CmdResult<()> {
    let channel = find_channel(&state, &channel_id)?;
    if let Ok(connector) = state.connectors.get(&channel.platform) {
        let _ = connector.disconnect(&channel).await;
    }
    state.db.delete_channel(&channel_id).map_err(err)?;
    let _ = state.automation.reload();
    Ok(())
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    channel_ids: Vec<String>,
    text: String,
) -> CmdResult<()> {
    let mut failures = Vec::new();
    for id in channel_ids {
        let channel = find_channel(&state, &id)?;
        let sent = match state.connectors.get(&channel.platform) {
            Ok(c) => c.send_message(&channel, &text).await,
            Err(e) => Err(e),
        };
        if let Err(e) = sent {
            failures.push(format!("{}: {e}", channel.display_name));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join(" · "))
    }
}

#[tauri::command]
pub async fn moderate(
    state: State<'_, AppState>,
    channel_id: String,
    kind: ModerationKind,
) -> CmdResult<()> {
    let channel = find_channel(&state, &channel_id)?;
    state
        .moderation
        .apply(&ModerationAction { channel, kind })
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn stream_infos(state: State<'_, AppState>) -> CmdResult<Vec<StreamInfo>> {
    let mut infos = Vec::new();
    let mut samples = Vec::new();
    let mut failed = false;
    for channel in state.db.list_channels().map_err(err)? {
        if let Ok(connector) = state.connectors.get(&channel.platform) {
            match connector.stream_info(&channel).await {
                Ok(Some(info)) => {
                    samples.push(ViewerSampleInput {
                        channel_id: channel.id.clone(),
                        platform: channel.platform.clone(),
                        viewers: info.viewers,
                    });
                    infos.push(info);
                }
                Ok(None) => {}
                Err(_) => failed = true,
            }
        }
    }
    // Un connecteur en erreur n'est pas « hors ligne » : on saute ce relevé plutôt que
    // d'enregistrer un creux factice ou de couper la session en deux.
    // L'historique est un bonus : une erreur d'écriture ne doit pas masquer l'état du live.
    if !failed {
        if let Err(e) = state.db.record_poll(now_ms(), &samples) {
            eprintln!("historique des sessions: {e}");
        }
    }
    Ok(infos)
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>) -> CmdResult<Vec<SessionSummary>> {
    state.db.list_sessions(200).map_err(err)
}

#[tauri::command]
pub fn session_detail(state: State<'_, AppState>, id: String) -> CmdResult<SessionDetail> {
    state
        .db
        .session_detail(&id)
        .map_err(err)?
        .ok_or_else(|| "session introuvable".to_string())
}

#[tauri::command]
/// Écrit le CSV dans le dossier Téléchargements (sinon le dossier de données) et renvoie le chemin.
/// Côté Rust plutôt qu'un téléchargement de la webview, peu fiable selon la plateforme.
pub fn save_session_csv(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<String> {
    let csv = state
        .db
        .export_session_csv(&id)
        .map_err(err)?
        .ok_or_else(|| "session introuvable".to_string())?;
    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe_id: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let path = dir.join(format!("omnistream-session-{safe_id}.csv"));
    // BOM UTF-8 pour qu'Excel lise correctement les accents.
    std::fs::write(&path, format!("\u{feff}{csv}")).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn delete_session(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_session(&id).map_err(err)
}

#[tauri::command]
pub async fn streaming_state(state: State<'_, AppState>) -> CmdResult<StreamingState> {
    state.streams.state().await.map_err(err)
}

#[tauri::command]
pub fn list_destinations(state: State<'_, AppState>) -> CmdResult<Vec<DestinationView>> {
    state.streams.list_destinations().map_err(err)
}

#[tauri::command]
pub async fn save_destination(
    state: State<'_, AppState>,
    destination: DestinationInput,
) -> CmdResult<DestinationView> {
    state
        .streams
        .save_destination(destination)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_destination(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.streams.delete_destination(&id).await.map_err(err)
}

#[tauri::command]
pub async fn go_live(state: State<'_, AppState>) -> CmdResult<()> {
    state.streams.go_live().await.map_err(err)
}

#[tauri::command]
pub async fn stop_live(state: State<'_, AppState>) -> CmdResult<()> {
    state.streams.stop_live().await;
    Ok(())
}

#[tauri::command]
pub async fn set_ingest_port(state: State<'_, AppState>, port: u16) -> CmdResult<()> {
    state.streams.set_ingest_port(port).await.map_err(err)
}

#[tauri::command]
pub async fn set_ffmpeg_path(state: State<'_, AppState>, path: String) -> CmdResult<FfmpegStatus> {
    state.streams.set_ffmpeg_path(&path).await.map_err(err)
}

#[derive(Serialize)]
pub struct TitleResult {
    platform: &'static str,
    ok: bool,
    message: String,
}

/// Mémorise le titre et l'applique là où une API le permet (Twitch pour l'instant).
#[tauri::command]
pub async fn set_stream_title(
    state: State<'_, AppState>,
    title: String,
) -> CmdResult<Vec<TitleResult>> {
    let title = title.trim().to_string();
    state.streams.save_title(&title).map_err(err)?;
    let mut results = Vec::new();
    if state.twitch.is_connected().await {
        let outcome = state.twitch.set_channel_title(&title).await;
        results.push(TitleResult {
            platform: "twitch",
            ok: outcome.is_ok(),
            message: outcome
                .map(|_| "Titre mis à jour".to_string())
                .unwrap_or_else(|e| e.to_string()),
        });
    }
    Ok(results)
}

/// Titre actuel de la chaîne Twitch connectée, pour préremplir le champ.
#[tauri::command]
pub async fn twitch_current_title(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    if !state.twitch.is_connected().await {
        return Ok(None);
    }
    state.twitch.channel_title().await.map(Some).map_err(err)
}

/// Applique les actions d'automod (timeout, suppression) sur la chaîne du message.
pub async fn apply_automod(app: &AppHandle, msg: &ChatMessage, verdict: &Verdict) {
    let state = app.state::<AppState>();
    let Ok(channel) = find_channel(&state, &msg.channel_id) else {
        return;
    };
    for result in state.moderation.apply_verdict(&channel, msg, verdict).await {
        if let Err(e) = result {
            eprintln!(
                "automod: action impossible sur {}: {e}",
                channel.display_name
            );
        }
    }
}

fn reload_automod(state: &AppState) -> CmdResult<()> {
    let rules = state.db.list_automod_rules().map_err(err)?;
    state.automod.lock().unwrap().set_rules(rules);
    Ok(())
}

#[tauri::command]
pub fn list_automod_rules(state: State<'_, AppState>) -> CmdResult<Vec<Rule>> {
    state.db.list_automod_rules().map_err(err)
}

#[tauri::command]
pub fn save_automod_rule(state: State<'_, AppState>, rule: Rule) -> CmdResult<()> {
    validate_rule(&rule).map_err(err)?;
    state.db.save_automod_rule(&rule).map_err(err)?;
    reload_automod(&state)
}

#[tauri::command]
pub fn delete_automod_rule(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_automod_rule(&id).map_err(err)?;
    reload_automod(&state)
}

/// Aperçu : quelles règles se déclencheraient sur ce texte (sans effet).
#[tauri::command]
pub fn test_automod(
    state: State<'_, AppState>,
    text: String,
    platform: Platform,
) -> CmdResult<Verdict> {
    let msg = ChatMessage {
        id: "test".into(),
        channel_id: "test".into(),
        platform,
        author: ChatAuthor {
            external_id: "test".into(),
            display_name: "test".into(),
            is_moderator: false,
            is_subscriber: false,
        },
        text,
        timestamp_ms: 0,
        emotes: vec![],
        badges: vec![],
        room_id: None,
    };
    Ok(state.automod.lock().unwrap().preview(&msg))
}

/// Applique titre / catégorie / tags aux chaînes choisies (toutes si `channel_ids` est vide) ;
/// renvoie un statut par chaîne, un échec n'annule pas les autres.
#[tauri::command]
pub async fn update_stream_info(
    state: State<'_, AppState>,
    channel_ids: Vec<String>,
    update: StreamInfoUpdate,
) -> CmdResult<Vec<PlatformUpdateResult>> {
    if update.is_empty() {
        return Err("Rien à modifier.".into());
    }
    let all = state.db.list_channels().map_err(err)?;
    let channels: Vec<Channel> = if channel_ids.is_empty() {
        all
    } else {
        all.into_iter()
            .filter(|c| channel_ids.contains(&c.id))
            .collect()
    };
    if channels.is_empty() {
        return Err("Aucune chaîne sélectionnée.".into());
    }
    Ok(state
        .connectors
        .update_stream_info(&channels, &update)
        .await)
}

#[tauri::command]
pub async fn search_categories(
    state: State<'_, AppState>,
    query: String,
) -> CmdResult<Vec<Category>> {
    let mut found: Vec<Category> = Vec::new();
    for platform in state.connectors.platforms() {
        if let Ok(connector) = state.connectors.get(&platform) {
            for cat in connector
                .search_categories(&query)
                .await
                .unwrap_or_default()
            {
                if !found.iter().any(|c| c.name.eq_ignore_ascii_case(&cat.name)) {
                    found.push(cat);
                }
            }
        }
    }
    Ok(found)
}

#[tauri::command]
pub fn list_presets(state: State<'_, AppState>) -> CmdResult<Vec<StreamPreset>> {
    state.db.list_presets().map_err(err)
}

#[tauri::command]
pub fn save_preset(state: State<'_, AppState>, preset: StreamPreset) -> CmdResult<()> {
    if preset.name.trim().is_empty() {
        return Err("Donnez un nom au préréglage.".into());
    }
    state.db.save_preset(&preset).map_err(err)
}

#[tauri::command]
pub fn delete_preset(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_preset(&id).map_err(err)
}

#[tauri::command]
pub fn list_rules(state: State<'_, AppState>) -> CmdResult<Vec<AutomationRule>> {
    state.db.list_rules().map_err(err)
}

#[tauri::command]
pub fn save_rule(state: State<'_, AppState>, rule: AutomationRule) -> CmdResult<()> {
    state.db.save_rule(&rule).map_err(err)?;
    state.automation.reload().map_err(err)
}

#[tauri::command]
pub fn delete_rule(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_rule(&id).map_err(err)?;
    state.automation.reload().map_err(err)
}

#[tauri::command]
pub fn list_chat_commands(state: State<'_, AppState>) -> CmdResult<Vec<ChatCommand>> {
    state.db.list_commands().map_err(err)
}

#[tauri::command]
pub fn save_chat_command(state: State<'_, AppState>, mut command: ChatCommand) -> CmdResult<()> {
    command.name = ChatCommand::normalize_name(&command.name);
    state.db.save_command(&command).map_err(err)?;
    state.automation.reload().map_err(err)
}

#[tauri::command]
pub fn delete_chat_command(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_command(&id).map_err(err)?;
    state.automation.reload().map_err(err)
}

#[tauri::command]
pub fn automation_log(state: State<'_, AppState>) -> Vec<LogEntry> {
    state.automation.recent_log()
}
