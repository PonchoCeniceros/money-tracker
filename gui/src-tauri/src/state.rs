use std::sync::Mutex;

use money_core::sync::production_backend;
use money_core::LedgerBackend;

/// Holds the single long-lived backend the GUI keeps open for its whole
/// process lifetime — unlike the CLI, which opens one per invocation. This is
/// the same `production_backend` the CLI uses: Supabase (with local mirror)
/// when remote config is present, plain local SQLite otherwise. The session
/// lives on this backend (in-memory access token refreshed from the keyring),
/// so the GUI and CLI can act on the same ledger interchangeably.
pub struct AppState {
    pub backend: Mutex<Box<dyn LedgerBackend>>,
    /// Email of the signed-in Supabase session, when one was started from the
    /// GUI (survives logout/relogin so the Settings panel can show it).
    pub session_email: Mutex<Option<String>>,
}

impl AppState {
    pub fn new() -> money_core::Result<Self> {
        let backend = production_backend(&money_core::Settings::load())?;
        Ok(Self {
            backend: Mutex::new(backend),
            session_email: Mutex::new(None),
        })
    }
}