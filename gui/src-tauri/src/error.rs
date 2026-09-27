use money_core::AppError;
use serde::Serialize;

/// `AppError` wraps non-`Serialize` errors (reqwest, io), so it can't cross the
/// Tauri IPC boundary directly. Every command returns this instead; `kind` is
/// what the frontend branches on (e.g. `not_configured` → Connect screen).
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub kind: String,
    pub message: String,
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        let kind = match &e {
            AppError::Io(_) => "io",
            AppError::Config(_) => "config",
            AppError::NotFound(_) => "not_found",
            AppError::Invalid(_) => "invalid",
            AppError::Remote(_) => "remote",
            AppError::Network(_) => "network",
            AppError::Auth(_) => "auth",
            AppError::InvalidGrant => "auth_needed",
            AppError::NotConfigured(_) => "not_configured",
            AppError::SchemaMismatch { .. } => "schema_mismatch",
        };
        ApiError {
            kind: kind.to_string(),
            message: e.to_string(),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
