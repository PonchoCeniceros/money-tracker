//! Local configuration file (`~/.money-tracker/config.toml`, mode 0600).
//!
//! Only two keys live here — `supabase_url` and `supabase_publishable_key` —
//! because those identify *where* the ledger lives and must survive across
//! CLI invocations and GUI sessions. Everything else (`emergency_pct`,
//! `default_account`, ...) stays in the ledger's `config` table so it is
//! replicated to Supabase like any other row.
//!
//! Env vars win over the file: `MONEY_TRACKER_SUPABASE_URL` and
//! `MONEY_TRACKER_SUPABASE_KEY` are checked first (the quickstart uses this
//! so a throwaway in a script never touches the user's real config).

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::db::db_path;

pub const SUPABASE_URL_ENV: &str = "MONEY_TRACKER_SUPABASE_URL";
pub const SUPABASE_KEY_ENV: &str = "MONEY_TRACKER_SUPABASE_KEY";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub supabase_url: Option<String>,
    pub supabase_publishable_key: Option<String>,
}

impl Settings {
    /// Loads persisted settings, then lets the environment override the two
    /// fields. Reads are never fallible here (a missing/unparseable file is
    /// the same as "no config"), so CLI/GUI startup doesn't need this to be
    /// `?`.
    pub fn load() -> Settings {
        let mut settings = read_file().unwrap_or_default();
        if let Ok(url) = std::env::var(SUPABASE_URL_ENV) {
            settings.supabase_url = if url.is_empty() { None } else { Some(url) };
        }
        if let Ok(key) = std::env::var(SUPABASE_KEY_ENV) {
            settings.supabase_publishable_key = if key.is_empty() { None } else { Some(key) };
        }
        settings
    }

    /// Whether remote mode is configured (either from env or the file).
    pub fn remote_configured(&self) -> bool {
        self.supabase_url.is_some()
    }

    /// Whether remote mode is both configured and has a key to authenticate
    /// with. A URL without a publishable key is a config error, not a
    /// no-op.
    pub fn is_complete(&self) -> bool {
        self.supabase_url.is_some() && self.supabase_publishable_key.is_some()
    }
}

fn read_file() -> Result<Settings> {
    let path = config_path();
    let bytes = fs::read(&path)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| AppError::Config(format!("{} is not valid UTF-8", path.display())))?;
    let parsed: Settings = toml::from_str(&text)
        .map_err(|e| AppError::Config(format!("{}: {e}", path.display())))?;
    Ok(parsed)
}

/// Overwrites the config file (creating `~/.money-tracker` if needed) with
/// the current `url`/`key`, chmod 0600 on POSIX. Used by `db remote login`.
pub fn save_settings(settings: &Settings) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let toml = toml::to_string(settings)
        .map_err(|e| AppError::Config(format!("cannot serialize settings: {e}")))?;
    let mut file = fs::File::create(&path)?;
    file.write_all(toml.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Clears url/key from the config file. Leaves other keys untouched.
pub fn clear_remote_config() -> Result<()> {
    let mut settings = read_file().unwrap_or_default();
    settings.supabase_url = None;
    settings.supabase_publishable_key = None;
    save_settings(&settings)
}

pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("MONEY_TRACKER_CONFIG") {
        return PathBuf::from(p);
    }
    config_dir().join("config.toml")
}

/// The `.money-tracker` directory under the user's home (or the parent of an
/// overridden `MONEY_TRACKER_CONFIG`). Shared by `config_path`, the mirror
/// path, and the auth token file fallback.
pub fn config_dir() -> PathBuf {
    if let Ok(p) = std::env::var("MONEY_TRACKER_CONFIG") {
        let path = PathBuf::from(p);
        return path
            .parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".money-tracker")
}

/// Resolves the local mirror database path. Defaults to the same file the
/// single-user app used before (so existing local data keeps working as the
/// mirror), where `MONEY_TRACKER_DB` points it at a throwaway for tests.
pub fn mirror_path() -> PathBuf {
    db_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_loads_defaults() {
        // Point config somewhere obviously nonexistent, then a relative
        // env that can't collide with anything.
        std::env::set_var("MONEY_TRACKER_CONFIG", "/nonexistent/x/config.toml");
        let s = Settings::load();
        assert!(s.supabase_url.is_none());
        std::env::remove_var("MONEY_TRACKER_CONFIG");
    }

    #[test]
    fn env_overrides_file() {
        std::env::set_var("MONEY_TRACKER_CONFIG", "/nonexistent/x/config.toml");
        std::env::set_var(SUPABASE_URL_ENV, "https://abc.supabase.co");
        let s = Settings::load();
        assert_eq!(s.supabase_url.as_deref(), Some("https://abc.supabase.co"));
        std::env::remove_var(SUPABASE_URL_ENV);
        std::env::remove_var("MONEY_TRACKER_CONFIG");
    }
}