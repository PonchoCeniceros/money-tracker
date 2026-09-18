use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config error: {0}")]
    Config(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    Invalid(String),
    #[error(
        "{path:?} uses the old schema (transactions/buckets), which this version cannot read.\n\n\
         This release replaces buckets and transactions with accounts and entries.\n\
         There is no automatic migration.\n\n\
         Back up and start clean:\n    \
         money-tracker db reset --backup\n"
    )]
    LegacySchema { path: PathBuf },
    #[error("Database schema version {found} is newer than this build supports ({expected}). Update money-tracker.")]
    SchemaTooNew { found: i32, expected: i32 },
    #[error("Database schema version {found} is older than this build supports and cannot be auto-upgraded.")]
    SchemaTooOld { found: i32 },
    /// A logic/state error surfaced by the remote (Supabase) backend — e.g.
    /// an upsert rejected by a trigger or CHECK constraint.
    #[error("Remote error: {0}")]
    Remote(String),
    /// Network/connection failure talking to Supabase.
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    /// Sign-in / session validation problem.
    #[error("Auth error: {0}")]
    Auth(String),
    /// The stored refresh token was revoked or expired ("invalid_grant"):
    /// caller must re-prompt for credentials rather than retry.
    #[error("Session expired — please log in again (db remote login)")]
    InvalidGrant,
}

pub type Result<T> = std::result::Result<T, AppError>;
