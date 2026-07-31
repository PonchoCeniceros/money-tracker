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
}

pub type Result<T> = std::result::Result<T, AppError>;
