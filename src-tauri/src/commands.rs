use omnistream_core::auth::twitch::{self as twitch_auth, DeviceCode};
use omnistream_core::connectors::TwitchCreds;
use omnistream_core::domain::{
    Account, Channel, ModerationAction, ModerationKind, Platform, StreamInfo,
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
    reconnect_channels(state).await;
}

async fn reconnect_channels(state: &AppState) {
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
    state.db.delete_account(&id).map_err(err)?;
    state.twitch.set_credentials(None).await;
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
    if platform != Platform::Twitch {
        return Err(format!("{platform} n'est pas encore pris en charge."));
    }
    let login = name.trim().trim_start_matches(['#', '@']).to_lowercase();
    if login.is_empty() || !login.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
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
    state.db.delete_channel(&channel_id).map_err(err)
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
    for channel in state.db.list_channels().map_err(err)? {
        if let Ok(connector) = state.connectors.get(&channel.platform) {
            if let Ok(Some(info)) = connector.stream_info(&channel).await {
                infos.push(info);
            }
        }
    }
    Ok(infos)
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
