use money_core::AppError;
use serde::Serialize;

/// `AppError` carries a `rusqlite::Error`, which isn't `Serialize`, so it
/// can't cross the Tauri IPC boundary directly. Every command returns this
/// instead — the conversion is a handler-layer concern, not the core's.
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub kind: String,
    pub message: String,
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        let kind = match &e {
            AppError::Database(_) => "database",
            AppError::Io(_) => "io",
            AppError::Config(_) => "config",
            AppError::NotFound(_) => "not_found",
            AppError::Invalid(_) => "invalid",
            AppError::LegacySchema { .. } => "legacy_schema",
            AppError::SchemaTooNew { .. } => "schema_too_new",
            AppError::SchemaTooOld { .. } => "schema_too_old",
        };
        ApiError {
            kind: kind.to_string(),
            message: e.to_string(),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
