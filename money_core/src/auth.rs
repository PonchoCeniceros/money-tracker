//! Supabase Auth client (GoTrue): email/password login, refresh, and token
//! persistence via OS keyring (file fallback). The access token lives only
//! in memory for the process lifetime; only the refresh token is stored.
//!
//! `InvalidGrant` maps to [`AppError::InvalidGrant`] so the CLI/GUI re-prompt
//! for credentials instead of blindly retrying.

use serde::Deserialize;

use crate::error::{AppError, Result};

const KEYRING_SERVICE: &str = "money-tracker";
const KEYRING_USER: &str = "supabase_refresh_token";
const FILE_FALLBACK: &str = "refresh_token";

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
}

/// A signed-in session. `access_token` is deliberately held here (memory
/// only); `refresh_token` is what gets persisted for recovery.
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
}

pub struct SupabaseAuth {
    url: String,
    publishable_key: String,
    client: reqwest::blocking::Client,
}

impl SupabaseAuth {
    pub fn new(url: &str, publishable_key: &str) -> Self {
        SupabaseAuth {
            url: url.trim_end_matches('/').to_string(),
            publishable_key: publishable_key.to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }

    pub fn publishable_key(&self) -> &str {
        &self.publishable_key
    }

    /// Authenticate with email + password. Returns the fresh session and
    /// persists the refresh token to the keyring.
    pub fn login(&self, email: &str, password: &str) -> Result<AuthSession> {
        let url = format!("{url}/auth/v1/token?grant_type=password", url = self.url);
        let resp = self
            .client
            .post(&url)
            .header("apikey", &self.publishable_key)
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()?;
        let session = parse_token_response(resp)?;
        persist_refresh_token(&session.refresh_token)?;
        Ok(session)
    }

    /// Exchange a stored refresh token for a new session (same credentials,
    /// no re-prompt). Keeps the new refresh token in the keyring.
    pub fn refresh(&self, refresh_token: &str) -> Result<AuthSession> {
        let url = format!("{url}/auth/v1/token?grant_type=refresh_token", url = self.url);
        let resp = self
            .client
            .post(&url)
            .header("apikey", &self.publishable_key)
            .json(&serde_json::json!({ "refresh_token": refresh_token }))
            .send()?;
        let session = parse_token_response(resp)?;
        persist_refresh_token(&session.refresh_token)?;
        Ok(session)
    }

    /// Load the persisted refresh token, if any.
    pub fn load_refresh_token(&self) -> Result<Option<String>> {
        load_refresh_token()
    }

    /// Clear the persisted refresh token (logout).
    pub fn clear_refresh_token(&self) -> Result<()> {
        clear_refresh_token()
    }
}

fn parse_token_response(resp: reqwest::blocking::Response) -> Result<AuthSession> {
    let status = resp.status();
    let body_text = resp.text().map_err(AppError::Network)?;
    if !status.is_success() {
        if let Ok(err) = serde_json::from_str::<serde_json::Value>(&body_text) {
            let msg = err
                .get("error_description")
                .or_else(|| err.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            if msg.to_lowercase().contains("invalid_grant")
                || msg.to_lowercase().contains("token has expired or is invalid")
            {
                return Err(AppError::InvalidGrant);
            }
            return Err(AppError::Auth(msg.to_string()));
        }
        return Err(AppError::Auth(format!("HTTP {status}: {body_text}")));
    }
    let parsed: TokenResponse = serde_json::from_str(&body_text)
        .map_err(|_| AppError::Auth("Malformed token response".into()))?;
    let refresh = parsed.refresh_token.ok_or_else(|| {
        AppError::Auth("Token response had no refresh token".into())
    })?;
    Ok(AuthSession {
        access_token: parsed.access_token,
        refresh_token: refresh,
    })
}

fn keyring() -> Result<keyring::Entry> {
    // Service/user combo straightforward; per-OS backends chosen by Cargo features.
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|_| AppError::Auth("keyring unavailable".into()))
}

fn file_fallback_path() -> std::path::PathBuf {
    crate::settings::config_dir().join(FILE_FALLBACK)
}

fn persist_refresh_token(token: &str) -> Result<()> {
    match keyring() {
        Ok(entry) => {
            if let Err(e) = entry.set_password(token) {
                // Keyring failure (e.g. headless session) — drop to a 0600
                // file rather than failing the login.
                save_file_fallback(token)
                    .map_err(|io| AppError::Auth(format!("keyring ({e}) + file fallback: {io}")))?;
            }
            Ok(())
        }
        Err(e) => save_file_fallback(token).map_err(|io| {
            AppError::Auth(format!("keyring ({e}) + file fallback: {io}"))
        }),
    }
}

fn save_file_fallback(token: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = file_fallback_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    f.write_all(token.as_bytes())?;
    Ok(())
}

fn load_refresh_token() -> Result<Option<String>> {
    if let Ok(entry) = keyring() {
        if let Ok(t) = entry.get_password() {
            return Ok(Some(t));
        }
        // otherwise fall through to file
    }
    let path = file_fallback_path();
    match std::fs::read_to_string(&path) {
        Ok(t) if !t.trim().is_empty() => Ok(Some(t.trim().to_string())),
        _ => Ok(None),
    }
}

fn clear_refresh_token() -> Result<()> {
    if let Ok(entry) = keyring() {
        let _ = entry.delete_credential();
    }
    let path = file_fallback_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}