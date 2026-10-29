//! Commandes Tauri propres à Trovo (OAuth « authorization code » avec redirection locale).

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::time::Duration;

use omnistream_core::auth::trovo as auth;
use omnistream_core::connectors::{TrovoAccount, TrovoApp};
use omnistream_core::domain::{Account, Platform};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

const CLIENT_ID_KEY: &str = "trovo_client_id";
const CLIENT_SECRET_KEY: &str = "trovo_client_secret";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

fn setting(state: &AppState, key: &str) -> Option<String> {
    state
        .db
        .get_setting(key)
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

fn app_config(state: &AppState) -> Option<TrovoApp> {
    // Le secret n'est nécessaire que pour la connexion : la lecture du chat n'exige que le Client ID.
    Some(TrovoApp {
        client_id: setting(state, CLIENT_ID_KEY)?,
        client_secret: setting(state, CLIENT_SECRET_KEY).unwrap_or_default(),
    })
}

/// Identifiant du compte en base, préfixé pour ne pas entrer en collision avec un id Twitch.
fn account_id(user_id: &str) -> String {
    format!("trovo:{user_id}")
}

/// Recharge l'application et le compte Trovo enregistrés (appelé au démarrage).
pub async fn restore(state: &AppState) {
    let Some(app) = app_config(state) else { return };
    state.trovo.set_app(Some(app)).await;
    let accounts = state.db.list_accounts().unwrap_or_default();
    if let Some(acc) = accounts.into_iter().find(|a| a.platform == Platform::Trovo) {
        if let Ok(Some(tokens)) = state.db.load_tokens(&acc.id) {
            state
                .trovo
                .set_account(Some(TrovoAccount {
                    user_id: acc.id,
                    login: acc.login,
                    tokens,
                }))
                .await;
        }
    }
}

#[derive(Serialize)]
pub struct TrovoSettings {
    client_id: Option<String>,
    has_secret: bool,
    redirect_uri: &'static str,
}

#[tauri::command]
pub fn get_trovo_settings(state: State<'_, AppState>) -> TrovoSettings {
    TrovoSettings {
        client_id: setting(&state, CLIENT_ID_KEY),
        has_secret: setting(&state, CLIENT_SECRET_KEY).is_some(),
        redirect_uri: auth::REDIRECT_URI,
    }
}

/// Enregistre le Client ID et le secret. Un secret vide conserve celui déjà enregistré.
#[tauri::command]
pub async fn set_trovo_app(
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
    state.trovo.set_app(app_config(&state)).await;
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

/// Démarre la connexion Trovo : renvoie l'adresse d'autorisation à ouvrir dans le navigateur,
/// puis attend la redirection en arrière-plan (événements `account://changed` / `account://error`).
#[tauri::command]
pub async fn trovo_login_start(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let cfg = app_config(&state)
        .filter(|c| !c.client_secret.is_empty())
        .ok_or("Renseignez d'abord le Client ID et le secret de votre application Trovo.")?;
    state.trovo.set_app(Some(cfg.clone())).await;
    let nonce = random_state();
    let url = auth::authorize_url(&cfg.client_id, &nonce);
    let http = state.trovo.http().clone();

    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let outcome: omnistream_core::Result<Account> = async {
            let code = auth::wait_for_code(&nonce, LOGIN_TIMEOUT).await?;
            let tokens =
                auth::exchange_code(&http, &cfg.client_id, &cfg.client_secret, &code).await?;
            let user = auth::fetch_user(&http, &cfg.client_id, &tokens.access_token).await?;
            let account = Account {
                id: account_id(&user.user_id),
                platform: Platform::Trovo,
                display_name: user.display_name,
                login: user.login.clone(),
            };
            // Un seul compte Trovo à la fois : on retire les précédents.
            for old in state
                .db
                .list_accounts()?
                .into_iter()
                .filter(|a| a.platform == Platform::Trovo && a.id != account.id)
            {
                state.db.delete_account(&old.id)?;
            }
            state.db.save_account(&account)?;
            state.db.save_tokens(&account.id, &tokens)?;
            state
                .trovo
                .set_account(Some(TrovoAccount {
                    user_id: account.id.clone(),
                    login: user.login,
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
