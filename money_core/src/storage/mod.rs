//! Storage layer: what a ledger *is* over two interchangeable stores.
//!
//! `LedgerBackend` is the port the whole crate talks to. The three
//! implementations are:
//! - [`sqlite::SqliteBackend`] — the local SQLite mirror, also the in-memory
//!   test store and the pre-Supabase local database.
//! - [`remote::SupabaseBackend`] — the hosted PostgREST backend.
//! - `sync::MirroringBackend` — remote-on-top, mirror-below (the production
//!   wiring).
//!
//! Balance/report math lives in [`ledger`] as pure functions over
//! [`Account`] + [`Entry`] rows — there is deliberately no "SELECT ... FROM
//! account_balances WHERE" anywhere except the two stores' `raw_accounts`
//! plumbing, so the two backends cannot drift from each other.

pub mod ledger;
pub mod remote;
pub mod sqlite;

use crate::error::{AppError, Result};
use crate::models::{
    Account, AccountBalance, Budget, Concept, Config, Entry, EntryFilter, EntryUpdate, NewAccount,
    NewEntry,
};

/// Banker's-rounding-free "cents" rounding used everywhere balances are
/// derived (mirrors SQLite's `ROUND(x, 2)` / Postgres `round(n, 2)`).
/// f64 is fine at personal-finance scale; values stay well below 2^53.
pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// A monotonic slice of the ledger, fetched from a backend since `cursor`
/// (exclusive). Applying it to the mirror advances `mirror_revision`.
#[derive(Debug, Clone, Default)]
pub struct LedgerDelta {
    /// The remote's `sync_state.revision` after these rows were fetched.
    /// Persisted by the mirror as its `mirror_revision`.
    pub cursor: i64,
    pub accounts: Vec<Account>,
    pub entries: Vec<Entry>,
    pub budgets: Vec<Budget>,
    pub concepts: Vec<Concept>,
    pub config: Vec<Config>,
    /// Row deletions since `cursor` (table name + primary key). Row-level
    /// cursors can't represent "this row no longer exists" on their own, so
    /// AFTER DELETE triggers on the remote record tombstones; the mirror
    /// replays them (a whitelist-guarded DELETE).
    pub deletes: Vec<(String, i64)>,
}

pub trait LedgerBackend: Send + Sync {
    // ------------------------------------------------------------------
    // Raw row access. These two are the only read primitives; everything
    // else (balances, reports) is derived in memory over them.
    // ------------------------------------------------------------------
    /// Accounts with no balance attached.
    fn raw_accounts(&self, include_archived: bool) -> Result<Vec<Account>>;
    /// Entries (with account-name columns filled, matching `entries_view`).
    fn entries(&self, f: &EntryFilter) -> Result<Vec<Entry>>;

    // ------------------------------------------------------------------
    // Writes. `push_entries` is the atomic batch path, used for every entry
    // write (single or multi, as with the emergency split or `setup`).
    // ------------------------------------------------------------------
    fn insert_account(&self, new: &NewAccount) -> Result<i64>;
    fn set_archived(&self, id: i64) -> Result<()>;
    fn push_entries(&self, entries: &[NewEntry]) -> Result<Vec<Entry>>;
    fn update_entry(&self, id: i64, upd: &EntryUpdate) -> Result<Entry>;
    fn delete_entry(&self, id: i64) -> Result<()>;
    fn get_entry(&self, id: i64) -> Result<Entry>;

    fn get_config(&self, key: &str) -> Result<Option<String>>;
    fn set_config(&self, key: &str, value: &str) -> Result<()>;
    fn list_config(&self) -> Result<Vec<Config>>;

    fn list_concepts(&self, type_filter: Option<&str>) -> Result<Vec<Concept>>;
    fn add_concept(&self, name: &str, concept_type: &str) -> Result<()>;

    fn set_budget(&self, concept: &str, limit: f64, period: &str) -> Result<()>;
    fn list_budgets(&self, period: Option<&str>) -> Result<Vec<Budget>>;
    fn delete_budget(&self, concept: &str, period: &str) -> Result<()>;

    // ------------------------------------------------------------------
    // Sync. Meaningful for SupabaseBackend (pull side) and SqliteBackend
    // (the mirror's apply side). Defaults error so a backend that isn't the
    // right side of a sync relationship says so instead of silently no-op.
    // ------------------------------------------------------------------
    fn remote_revision(&self) -> Result<i64> {
        Err(AppError::Remote(
            "this backend has no remote revision (sync is not configured)".into(),
        ))
    }
    fn pull_changes_since(&self, _cursor: i64) -> Result<LedgerDelta> {
        Err(AppError::Remote(
            "this backend is not readable as a sync source".into(),
        ))
    }
    fn apply_remote_snapshot(&self, _delta: &LedgerDelta) -> Result<()> {
        Err(AppError::Remote(
            "this backend is not a sync mirror".into(),
        ))
    }
    /// The mirror's highest applied remote revision (0 when empty/fresh).
    fn sync_cursor(&self) -> Result<i64> {
        Err(AppError::Remote(
            "this backend does not track a sync cursor".into(),
        ))
    }
    /// Clears the mirror cursor to 0 — used by `db remote migrate` when a
    /// mirror is rebuilt from a snapshot.
    fn reset_mirror_cursor(&self) -> Result<()> {
        Err(AppError::Remote(
            "this backend does not track a sync cursor".into(),
        ))
    }
    /// One sync round: pull remote changes since the mirror cursor and apply
    /// them to the mirror. Meaningful on a remote+mirror pairing
    /// ([`MirroringBackend`]); errors when the backend isn't a sync source so
    /// the caller can treat it as "sync not configured".
    fn poll_sync(&self) -> Result<()> {
        let cursor = self.sync_cursor()?;
        let delta = self.pull_changes_since(cursor)?;
        self.apply_remote_snapshot(&delta)
    }
    /// Last non-fatal mirror write warning (e.g. the mirror fell behind but
    /// the remote write succeeded). Consumed by CLI/GUI for a visible note.
    fn take_sync_warning(&self) -> Option<String> {
        None
    }

    // ------------------------------------------------------------------
    // Derived reads — the single source of truth for balances/reports.
    // Each default is defined purely over `raw_accounts` + `entries`, so
    // both backends produce identical numbers by construction.
    // ------------------------------------------------------------------
    fn accounts_with_balances(
        &self,
        include_archived: bool,
        up_to_date: Option<&str>,
    ) -> Result<Vec<AccountBalance>> {
        let accounts = self.raw_accounts(include_archived)?;
        let f = EntryFilter {
            up_to_date: up_to_date.map(|s| s.to_string()),
            ..Default::default()
        };
        let entries = self.entries(&f)?;
        Ok(ledger::derive_balances(&accounts, &entries))
    }

    fn list_accounts(&self, include_archived: bool) -> Result<Vec<AccountBalance>> {
        self.accounts_with_balances(include_archived, None)
    }

    fn get_account(&self, id: i64) -> Result<AccountBalance> {
        let mut accounts = self.raw_accounts(true)?;
        let idx = accounts
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| AppError::NotFound(format!("Account #{id} not found")))?;
        let account = accounts.swap_remove(idx);
        let entries = self.entries(&EntryFilter::default())?;
        Ok(ledger::derive_balances(&[account], &entries)
            .into_iter()
            .next()
            .unwrap())
    }

    fn find_account_by_name(&self, name: &str) -> Result<Option<AccountBalance>> {
        let mut accounts = self.raw_accounts(true)?;
        let idx = match accounts.iter().position(|a| a.name == name) {
            Some(i) => i,
            None => return Ok(None),
        };
        let account = accounts.swap_remove(idx);
        let entries = self.entries(&EntryFilter::default())?;
        Ok(ledger::derive_balances(&[account], &entries)
            .into_iter()
            .next())
    }

    fn require_account_by_name(&self, name: &str) -> Result<AccountBalance> {
        self.find_account_by_name(name)?
            .ok_or_else(|| AppError::NotFound(format!("Account '{name}' not found")))
    }

    fn emergency_account(&self) -> Result<Option<AccountBalance>> {
        let mut accounts = self.raw_accounts(true)?;
        let idx = match accounts
            .iter()
            .position(|a| a.kind == crate::models::AccountKind::Emergency && !a.archived)
        {
            Some(i) => i,
            None => return Ok(None),
        };
        let account = accounts.swap_remove(idx);
        let entries = self.entries(&EntryFilter::default())?;
        Ok(ledger::derive_balances(&[account], &entries)
            .into_iter()
            .next())
    }

    fn archive_account(&self, id: i64, force: bool) -> Result<()> {
        let account = self.get_account(id)?;
        if !force && account.balance.abs() > 0.005 {
            return Err(AppError::Invalid(format!(
                "'{}' has a balance of ${:.2}. Empty it first or pass --force",
                account.name, account.balance
            )));
        }
        self.set_archived(id)
    }

    fn balance_as_of(&self, id: i64, date: &str) -> Result<f64> {
        let f = EntryFilter {
            up_to_date: Some(date.to_string()),
            ..Default::default()
        };
        let entries = self.entries(&f)?;
        Ok(round2(entries.iter().map(|e| e.delta_for(id)).sum()))
    }

    fn is_seeded(&self) -> Result<bool> {
        let f = EntryFilter {
            limit: Some(1),
            ..Default::default()
        };
        Ok(!self.entries(&f)?.is_empty())
    }
}