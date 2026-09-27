use serde::{Deserialize, Serialize};

use crate::models::{Account, Budget, Concept, Config, Entry};

/// Result of the `ledger_status()` RPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct LedgerStatus {
    /// Global revision: goes up on every insert, update and delete.
    #[cfg_attr(feature = "ts-rs", ts(type = "number"))]
    pub revision: i64,
    #[cfg_attr(feature = "ts-rs", ts(type = "number"))]
    pub schema_version: i64,
}

/// The whole ledger at one point in time (the `export_ledger()` RPC), for backups.
/// `accounts` includes archived ones; `entries` carry ids but no account names.
#[derive(Debug, Clone, Default)]
pub struct LedgerSnapshot {
    pub revision: i64,
    pub schema_version: i64,
    /// As reported by the database (RFC 3339).
    pub exported_at: String,
    pub concepts: Vec<Concept>,
    pub accounts: Vec<Account>,
    pub entries: Vec<Entry>,
    pub budgets: Vec<Budget>,
    pub config: Vec<Config>,
}

/// What the CLI and GUI show after a backup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct BackupInfo {
    pub path: String,
    /// Local time, RFC 3339.
    pub created_at: String,
    #[cfg_attr(feature = "ts-rs", ts(type = "number"))]
    pub revision: i64,
    #[cfg_attr(feature = "ts-rs", ts(type = "number"))]
    pub schema_version: i64,
    #[cfg_attr(feature = "ts-rs", ts(type = "number"))]
    pub entries: i64,
    pub automatic: bool,
}
