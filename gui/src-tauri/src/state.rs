use std::ops::Deref;
use std::sync::{Mutex, MutexGuard};

use money_core::{LedgerBackend, Settings};

use crate::error::ApiResult;

/// The GUI keeps one long-lived Supabase backend for its whole process
/// (the CLI opens one per invocation). It is built on first use, not at
/// startup, so a missing config or session reaches the frontend as an error
/// (`not_configured`, `auth_needed`, `schema_mismatch`) instead of a panic
/// before the window opens.
pub struct AppState {
    backend: Mutex<Option<Box<dyn LedgerBackend>>>,
}

/// Lock on a connected backend; derefs to `Box<dyn LedgerBackend>`.
pub struct BackendGuard<'a>(MutexGuard<'a, Option<Box<dyn LedgerBackend>>>);

impl Deref for BackendGuard<'_> {
    type Target = Box<dyn LedgerBackend>;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("AppState::backend connects before returning a guard")
    }
}

impl AppState {
    pub fn new() -> Self {
        AppState { backend: Mutex::new(None) }
    }

    /// The connected backend, connecting first if needed.
    pub fn backend(&self) -> ApiResult<BackendGuard<'_>> {
        let mut guard = self.backend.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(money_core::storage::connect(&Settings::load())?);
        }
        Ok(BackendGuard(guard))
    }

    /// Drop the connection (after login/logout or a config change) so the next
    /// call reconnects with the current settings and session.
    pub fn reset(&self) {
        *self.backend.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
