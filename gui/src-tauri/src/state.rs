use std::sync::Arc;

use money_core::{LedgerBackend, Settings};
use tauri::async_runtime::Mutex;

use crate::error::{ApiError, ApiResult};

/// A shared, connected backend. `SupabaseBackend` is `Send + Sync` (its session
/// lives behind its own lock), so every command can use it at the same time.
pub type SharedBackend = Arc<Box<dyn LedgerBackend>>;

/// The GUI keeps one long-lived Supabase backend for its whole process (the
/// CLI opens one per invocation). It is built on first use, not at startup, so
/// a missing config or session reaches the frontend as an error
/// (`not_configured`, `auth_needed`, `schema_mismatch`) instead of a panic
/// before the window opens.
///
/// Two rules keep this fast and panic-free:
/// - The lock is held only to read the `Arc` (or, once, while connecting), never
///   during a command's network calls, so the frontend's parallel requests run in
///   parallel.
/// - A backend is never dropped on an async thread: it owns a blocking HTTP client
///   whose inner runtime panics if dropped there ("Cannot drop a runtime in a
///   context where blocking is not allowed"). Commands move their clone into
///   [`blocking`], and [`AppState::reset`] drops the old one on a plain thread.
pub struct AppState {
    backend: Mutex<Option<SharedBackend>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState { backend: Mutex::new(None) }
    }

    /// The connected backend, connecting first (off the UI thread) if needed.
    /// Concurrent first calls wait for a single connection instead of racing.
    pub async fn backend(&self) -> ApiResult<SharedBackend> {
        let mut slot = self.backend.lock().await;
        if let Some(be) = slot.as_ref() {
            return Ok(be.clone());
        }
        let be: SharedBackend = Arc::new(
            blocking(|| Ok(money_core::storage::connect(&Settings::load())?)).await?,
        );
        *slot = Some(be.clone());
        Ok(be)
    }

    /// Drop the connection (after login/logout or a config change) so the next
    /// call reconnects with the current settings and session.
    pub async fn reset(&self) {
        if let Some(old) = self.backend.lock().await.take() {
            std::thread::spawn(move || drop(old));
        }
    }
}

/// Runs blocking work (every Supabase call is a blocking HTTP request) on
/// Tauri's blocking thread pool. Synchronous Tauri commands run on the main
/// thread, where a network wait freezes the whole window.
pub async fn blocking<T, F>(f: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ApiResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| ApiError {
        kind: "internal".into(),
        message: format!("background task failed: {e}"),
    })?
}
