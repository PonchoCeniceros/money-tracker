//! Sync + session commands: `sync_poll` drives one mirror refresh (called by
//! the frontend's `useSync` timer), `sync_status` describes remote mode and
//! parity for the Settings panel, and the auth commands route through
//! `money_core::auth` so the CLI and GUI share the same stored session.
use tauri::State;

use money_core::auth::SupabaseAuth;
use money_core::Settings;

use crate::error::ApiResult;
use crate::state::AppState;

/// Mirrors `production_backend`'s split: sync only exists when remote config
/// is present, so these return a uniform "sync not configured" state instead
/// of erroring on the plain-local mode.
fn auth_from_settings() -> ApiResult<SupabaseAuth> {
    let s = Settings::load();
    let url = s
        .supabase_url
        .ok_or_else(|| money_core::AppError::Config("Supabase URL no configurada".into()))?;
    let key = s
        .supabase_publishable_key
        .ok_or_else(|| money_core::AppError::Config("Publishable key no configurada".into()))?;
    Ok(SupabaseAuth::new(&url, &key))
}

#[derive(serde::Serialize)]
pub struct SyncStatus {
    pub remote_configured: bool,
    pub logged_in: bool,
    pub email: Option<String>,
    pub remote_revision: Option<i64>,
    pub mirror_cursor: Option<i64>,
    pub warning: Option<String>,
}

#[tauri::command]
pub fn sync_status(state: State<AppState>) -> ApiResult<SyncStatus> {
    let s = Settings::load();
    let mut logged_in = false;
    if let Ok(auth) = auth_from_settings() {
        logged_in = auth.load_refresh_token()?.is_some();
    }

    let be = state.backend.lock().unwrap();
    let remote_revision = be.remote_revision().ok();
    let mirror_cursor = be.sync_cursor().ok();
    let warning = be.take_sync_warning();

    Ok(SyncStatus {
        remote_configured: s.is_complete(),
        logged_in,
        email: state.session_email.lock().unwrap().clone(),
        remote_revision,
        mirror_cursor,
        warning,
    })
}

/// Pull remote changes into the mirror. Returns the resulting mirror cursor
/// and the sync warning, if any — sync itself is best-effort, so a mirror
/// that fell behind surfaces here instead of blocking the caller. Errors when
/// remote mode isn't configured.
#[derive(serde::Serialize)]
pub struct SyncPollResult {
    pub cursor: Option<i64>,
    pub warning: Option<String>,
}

#[tauri::command]
pub fn sync_poll(state: State<AppState>) -> ApiResult<SyncPollResult> {
    let be = state.backend.lock().unwrap();
    be.poll_sync()?;
    Ok(SyncPollResult {
        cursor: be.sync_cursor().ok(),
        warning: be.take_sync_warning(),
    })
}

#[derive(serde::Deserialize)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

#[tauri::command]
pub fn remote_login(state: State<AppState>, input: LoginInput) -> ApiResult<()> {
    let auth = auth_from_settings()?;
    auth.login(&input.email, &input.password)?;
    *state.session_email.lock().unwrap() = Some(input.email);
    Ok(())
}

#[tauri::command]
pub fn remote_logout(state: State<AppState>) -> ApiResult<()> {
    let auth = match auth_from_settings() {
        Ok(a) => a,
        Err(_) => SupabaseAuth::new("", ""),
    };
    auth.clear_refresh_token()?;
    *state.session_email.lock().unwrap() = None;
    Ok(())
}