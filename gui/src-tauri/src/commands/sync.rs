//! Connection and session commands. `ledger_status` is what the frontend polls
//! (~30 s) to know when to refetch, and what gates the Connect screen: its
//! error kind (`not_configured`, `auth_needed`, `schema_mismatch`) says why.
use tauri::State;

use money_core::auth::SupabaseAuth;
use money_core::models::LedgerStatus;
use money_core::services::backup_service::{self, LastBackupRecord};
use money_core::settings::save_settings;
use money_core::Settings;

use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub fn ledger_status(state: State<AppState>) -> ApiResult<LedgerStatus> {
    let be = state.backend()?;
    Ok(be.status()?)
}

#[derive(serde::Serialize)]
pub struct ConnectionInfo {
    pub url: Option<String>,
    pub configured: bool,
    pub logged_in: bool,
    pub email: Option<String>,
    /// "llavero del sistema" or "archivo".
    pub token_storage: String,
    pub last_backup: Option<LastBackupRecord>,
}

/// What Settings shows about the connection. Never fails: an unreachable or
/// unconfigured Supabase is reported as flags, not as an error.
#[tauri::command]
pub fn connection_info(state: State<AppState>) -> ApiResult<ConnectionInfo> {
    let s = Settings::load();
    let configured = s.is_complete();
    let auth = SupabaseAuth::new(
        s.supabase_url.as_deref().unwrap_or_default(),
        s.supabase_publishable_key.as_deref().unwrap_or_default(),
    );
    let logged_in = configured && auth.load_refresh_token()?.is_some();
    let email = if logged_in {
        state.backend().ok().and_then(|be| be.session_email())
    } else {
        None
    };
    Ok(ConnectionInfo {
        url: s.supabase_url.clone(),
        configured,
        logged_in,
        email,
        token_storage: auth.storage().label().to_string(),
        last_backup: backup_service::last_backup(),
    })
}

#[derive(serde::Deserialize)]
pub struct LoginInput {
    /// Only needed the first time (or to switch projects); otherwise the saved ones are used.
    pub url: Option<String>,
    pub key: Option<String>,
    pub email: String,
    pub password: String,
}

#[tauri::command]
pub fn remote_login(state: State<AppState>, input: LoginInput) -> ApiResult<()> {
    let mut settings = Settings::load();
    let url = input
        .url
        .filter(|u| !u.trim().is_empty())
        .or(settings.supabase_url.clone())
        .ok_or_else(|| money_core::AppError::Config("Falta la URL del proyecto de Supabase".into()))?;
    let key = input
        .key
        .filter(|k| !k.trim().is_empty())
        .or(settings.supabase_publishable_key.clone())
        .ok_or_else(|| money_core::AppError::Config("Falta la publishable key".into()))?;

    SupabaseAuth::new(&url, &key).login(&input.email, &input.password)?;

    settings.supabase_url = Some(url);
    settings.supabase_publishable_key = Some(key);
    save_settings(&settings)?;
    state.reset();
    Ok(())
}

#[tauri::command]
pub fn remote_logout(state: State<AppState>) -> ApiResult<()> {
    let s = Settings::load();
    SupabaseAuth::new(
        s.supabase_url.as_deref().unwrap_or_default(),
        s.supabase_publishable_key.as_deref().unwrap_or_default(),
    )
    .clear_refresh_token()?;
    state.reset();
    Ok(())
}
