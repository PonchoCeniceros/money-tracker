use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config error: {0}")]
    Config(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    Invalid(String),
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
    /// Supabase URL or publishable key missing; the message says how to set them.
    #[error("{0}")]
    NotConfigured(String),
    /// The database's `schema_version` isn't the one this build expects.
    #[error("{}", schema_mismatch_message(*found, *expected))]
    SchemaMismatch { found: i64, expected: i64 },
}

fn schema_mismatch_message(found: i64, expected: i64) -> String {
    let head = format!(
        "El esquema de Supabase está en la versión {found} y esta app espera la {expected}."
    );
    if found < expected {
        let files = if expected - found == 1 {
            format!("supabase/sql/{expected:04}_*.sql")
        } else {
            format!("en orden supabase/sql/{:04}_*.sql a {expected:04}_*.sql", found + 1)
        };
        format!("{head} Aplica {files} en el SQL Editor (ver supabase/README.md).")
    } else {
        format!("{head} Actualiza la app (README, sección 1.6).")
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
