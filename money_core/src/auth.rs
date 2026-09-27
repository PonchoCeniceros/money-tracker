//! Supabase Auth client (GoTrue): email/password login, refresh, and refresh-token
//! persistence. The access token lives only in memory for the process lifetime.
//!
//! Where the refresh token goes depends on `Settings::token_storage()`:
//! - `Keychain` (default): the OS keychain, one entry per Supabase project
//!   (`supabase_refresh_token:<project-ref>`), falling back to a 0600 file when the
//!   keychain is unavailable (headless sessions).
//! - `File`: only the 0600 file under the config dir. No code path touches the
//!   keychain, because any access makes macOS prompt for the login password.
//!
//! `InvalidGrant` maps to [`AppError::InvalidGrant`] so the CLI/GUI re-prompt
//! for credentials instead of blindly retrying.

use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::settings::{Settings, TokenStorage};

const KEYRING_SERVICE: &str = "money-tracker";
/// Pre-002 entry, shared by every project. Only removed on logout, never read.
const LEGACY_KEYRING_USER: &str = "supabase_refresh_token";
const FILE_NAME: &str = "refresh_token";

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub user: Option<SessionUser>,
}

/// The signed-in user, as returned by GoTrue with every token.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SessionUser {
    pub id: String,
    pub email: Option<String>,
}

/// A signed-in session. `access_token` is deliberately held here (memory
/// only); `refresh_token` is what gets persisted for recovery.
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub user: Option<SessionUser>,
}

pub struct SupabaseAuth {
    url: String,
    publishable_key: String,
    storage: TokenStorage,
    client: reqwest::blocking::Client,
}

impl SupabaseAuth {
    /// Uses the token storage configured in `config.toml`.
    pub fn new(url: &str, publishable_key: &str) -> Self {
        Self::with_storage(url, publishable_key, Settings::load().token_storage())
    }

    pub fn with_storage(url: &str, publishable_key: &str, storage: TokenStorage) -> Self {
        SupabaseAuth {
            url: url.trim_end_matches('/').to_string(),
            publishable_key: publishable_key.to_string(),
            storage,
            client: reqwest::blocking::Client::new(),
        }
    }

    pub fn publishable_key(&self) -> &str {
        &self.publishable_key
    }

    pub fn storage(&self) -> TokenStorage {
        self.storage
    }

    /// Authenticate with email + password. Returns the fresh session and
    /// persists the refresh token.
    pub fn login(&self, email: &str, password: &str) -> Result<AuthSession> {
        let url = format!("{url}/auth/v1/token?grant_type=password", url = self.url);
        let resp = self
            .client
            .post(&url)
            .header("apikey", &self.publishable_key)
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()?;
        let session = parse_token_response(resp)?;
        self.persist(&session.refresh_token)?;
        Ok(session)
    }

    /// Exchange a stored refresh token for a new session (same credentials,
    /// no re-prompt). Supabase rotates the refresh token, so the new one is stored.
    pub fn refresh(&self, refresh_token: &str) -> Result<AuthSession> {
        let url = format!("{url}/auth/v1/token?grant_type=refresh_token", url = self.url);
        let resp = self
            .client
            .post(&url)
            .header("apikey", &self.publishable_key)
            .json(&serde_json::json!({ "refresh_token": refresh_token }))
            .send()?;
        let session = parse_token_response(resp)?;
        self.persist(&session.refresh_token)?;
        Ok(session)
    }

    /// Load the persisted refresh token, if any.
    pub fn load_refresh_token(&self) -> Result<Option<String>> {
        if self.storage == TokenStorage::Keychain {
            if let Ok(entry) = self.keyring_entry() {
                if let Ok(t) = entry.get_password() {
                    return Ok(Some(t));
                }
            }
        }
        match std::fs::read_to_string(token_file()) {
            Ok(t) if !t.trim().is_empty() => Ok(Some(t.trim().to_string())),
            _ => Ok(None),
        }
    }

    /// Forget the session (logout). In keychain mode also removes the pre-002
    /// shared entry, so an upgrade leaves nothing stale behind.
    pub fn clear_refresh_token(&self) -> Result<()> {
        if self.storage == TokenStorage::Keychain {
            if let Ok(entry) = self.keyring_entry() {
                let _ = entry.delete_credential();
            }
            if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, LEGACY_KEYRING_USER) {
                let _ = entry.delete_credential();
            }
        }
        let path = token_file();
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn persist(&self, token: &str) -> Result<()> {
        if self.storage == TokenStorage::Keychain {
            match self.keyring_entry().and_then(|e| {
                e.set_password(token).map_err(|err| AppError::Auth(err.to_string()))
            }) {
                Ok(()) => return Ok(()),
                // Keychain unavailable (e.g. headless session): drop to the file.
                Err(e) => {
                    return save_token_file(token)
                        .map_err(|io| AppError::Auth(format!("keyring ({e}) + file fallback: {io}")))
                }
            }
        }
        save_token_file(token).map_err(|io| AppError::Auth(format!("token file: {io}")))
    }

    fn keyring_entry(&self) -> Result<keyring::Entry> {
        let user = format!("{LEGACY_KEYRING_USER}:{}", project_ref(&self.url));
        keyring::Entry::new(KEYRING_SERVICE, &user)
            .map_err(|_| AppError::Auth("keyring unavailable".into()))
    }
}

/// `https://abcd.supabase.co` → `abcd`; anything else (e.g. a local
/// `http://127.0.0.1:54321`) → its host and port with separators replaced.
pub fn project_ref(url: &str) -> String {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or_default();
    match host.strip_suffix(".supabase.co") {
        Some(r) => r.to_string(),
        None => host.replace([':', '.'], "_"),
    }
}

fn parse_token_response(resp: reqwest::blocking::Response) -> Result<AuthSession> {
    let status = resp.status();
    let body_text = resp.text().map_err(AppError::Network)?;
    if !status.is_success() {
        if let Ok(err) = serde_json::from_str::<serde_json::Value>(&body_text) {
            let msg = err
                .get("error_description")
                .or_else(|| err.get("msg"))
                .or_else(|| err.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            if msg.to_lowercase().contains("invalid_grant")
                || msg.to_lowercase().contains("token has expired or is invalid")
                || msg.to_lowercase().contains("refresh token not found")
            {
                return Err(AppError::InvalidGrant);
            }
            return Err(AppError::Auth(msg.to_string()));
        }
        return Err(AppError::Auth(format!("HTTP {status}: {body_text}")));
    }
    let parsed: TokenResponse = serde_json::from_str(&body_text)
        .map_err(|_| AppError::Auth("Malformed token response".into()))?;
    let refresh = parsed
        .refresh_token
        .ok_or_else(|| AppError::Auth("Token response had no refresh token".into()))?;
    Ok(AuthSession {
        access_token: parsed.access_token,
        refresh_token: refresh,
        user: parsed.user,
    })
}

fn token_file() -> std::path::PathBuf {
    crate::settings::config_dir().join(FILE_NAME)
}

fn save_token_file(token: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = token_file();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_ref_separates_projects() {
        assert_eq!(project_ref("https://dmiolszbteuckdoembnc.supabase.co"), "dmiolszbteuckdoembnc");
        assert_eq!(project_ref("https://abc.supabase.co/"), "abc");
        assert_eq!(project_ref("http://127.0.0.1:54321"), "127_0_0_1_54321");
        assert_ne!(
            project_ref("https://dmiolszbteuckdoembnc.supabase.co"),
            project_ref("http://127.0.0.1:54321")
        );
    }

    #[test]
    fn token_response_carries_the_user() {
        let body = r#"{"access_token":"a","refresh_token":"r","expires_in":3600,
                       "user":{"id":"8f1c","email":"yo@example.com","role":"authenticated"}}"#;
        let parsed: TokenResponse = serde_json::from_str(body).unwrap();
        assert_eq!(
            parsed.user,
            Some(SessionUser { id: "8f1c".into(), email: Some("yo@example.com".into()) })
        );
    }
}
