use std::sync::Mutex;

use rusqlite::Connection;

/// Holds the single long-lived connection the GUI keeps open for its whole
/// process lifetime — unlike the CLI, which opens one connection per
/// invocation and closes it on exit. WAL mode (already on via `open_db`)
/// lets the CLI and GUI run against the same database file concurrently.
pub struct AppState {
    pub conn: Mutex<Connection>,
}

impl AppState {
    pub fn new() -> money_core::Result<Self> {
        let conn = money_core::db::open_db()?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}
