//! Commandes Tauri propres à Kick (OAuth « authorization code » avec redirection locale).

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::time::Duration;

use omnistream_core::auth::kick as auth;
use omnistream_core::connectors::{KickAccount, KickApp};
use omnistream_core::domain::{Account, Platform};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

const CLIENT_ID_KEY: &str = "kick_client_id";
const CLIENT_SECRET_KEY: &str = "kick_client_secret";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

fn setting(state: &AppState, key: &str) -> Option<String> {
    state
        .db
        .get_setting(key)
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

fn app_config(state: &AppState) -> Option<KickApp> {
    Some(KickApp {
        client_id: setting(state, CLIENT_ID_KEY)?,
        client_secret: setting(state, CLIENT_SECRET_KEY)?,
    })
}

/// Identifiant du compte en base, préfixé pour ne pas entrer en collision avec un id Twitch.
fn account_id(user_id: &str) -> String {
    format!("kick:{user_id}")
}

/// Recharge l'application et le compte Kick enregistrés (appelé au démarrage).
pub async fn restore(state: &AppState) {
    let Some(app) = app_config(state) else { return };
    state.kick.set_app(Some(app)).await;
    let accounts = state.db.list_accounts().unwrap_or_default();
    if let Some(acc) = accounts.into_iter().find(|a| a.platform == Platform::Kick) {
        if let Ok(Some(tokens)) = state.db.load_tokens(&acc.id) {
            state
                .kick
                .set_account(Some(KickAccount {
                    user_id: acc.id,
                    tokens,
                }))
                .await;
        }
    }
}

#[derive(Serialize)]
pub struct KickSettings {
    client_id: Option<String>,
    has_secret: bool,
    redirect_uri: &'static str,
}

#[tauri::command]
pub fn get_kick_settings(state: State<'_, AppState>) -> KickSettings {
    KickSettings {
        client_id: setting(&state, CLIENT_ID_KEY),
        has_secret: setting(&state, CLIENT_SECRET_KEY).is_some(),
        redirect_uri: auth::REDIRECT_URI,
    }
}

/// Enregistre le Client ID et le secret. Un secret vide conserve celui déjà enregistré.
#[tauri::command]
pub async fn set_kick_app(
    state: State<'_, AppState>,
    client_id: String,
    client_secret: String,
) -> Result<(), String> {
    state
        .db
        .set_setting(CLIENT_ID_KEY, client_id.trim())
        .map_err(|e| e.to_string())?;
    if !client_secret.trim().is_empty() {
        state
            .db
            .set_setting(CLIENT_SECRET_KEY, client_secret.trim())
            .map_err(|e| e.to_string())?;
    }
    state.kick.set_app(app_config(&state)).await;
    Ok(())
}

fn random_state() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Deux hachages à clés aléatoires distinctes (RandomState est initialisé par l'OS).
    let part = || {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(nanos);
        h.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

/// Démarre la connexion Kick : renvoie l'adresse d'autorisation à ouvrir dans le navigateur,
/// puis attend la redirection en arrière-plan (événements `account://changed` / `account://error`).
#[tauri::command]
pub async fn kick_login_start(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let cfg = app_config(&state)
        .filter(|c| !c.client_secret.is_empty())
        .ok_or("Renseignez d'abord le Client ID et le secret de votre application Kick.")?;
    state.kick.set_app(Some(cfg.clone())).await;
    let nonce = random_state();
    let (verifier, challenge) = auth::new_pkce().map_err(|e| e.to_string())?;
    let url = auth::authorize_url(&cfg.client_id, &nonce, &challenge);
    let http = state.kick.http().clone();

    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let outcome: omnistream_core::Result<Account> = async {
            let code = auth::wait_for_code(&nonce, LOGIN_TIMEOUT).await?;
            let tokens =
                auth::exchange_code(&http, &cfg.client_id, &cfg.client_secret, &code, &verifier)
                    .await?;
            let user = auth::fetch_user(&http, &tokens.access_token).await?;
            let account = Account {
                id: account_id(&user.user_id),
                platform: Platform::Kick,
                display_name: user.name.clone(),
                login: user.name,
            };
            // Un seul compte Kick à la fois : on retire les précédents.
            for old in state
                .db
                .list_accounts()?
                .into_iter()
                .filter(|a| a.platform == Platform::Kick && a.id != account.id)
            {
                state.db.delete_account(&old.id)?;
            }
            state.db.save_account(&account)?;
            state.db.save_tokens(&account.id, &tokens)?;
            state
                .kick
                .set_account(Some(KickAccount {
                    user_id: account.id.clone(),
                    tokens,
                }))
                .await;
            Ok(account)
        }
        .await;
        match outcome {
            Ok(account) => {
                crate::commands::reconnect_channels(&state).await;
                let _ = app.emit("account://changed", account);
            }
            Err(e) => {
                let _ = app.emit("account://error", e.to_string());
            }
        }
    });
    Ok(url)
}
