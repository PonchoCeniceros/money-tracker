//! Storage layer: the port the whole crate talks to.
//!
//! `LedgerBackend` only stores and reads; accounting rules live in `crate::rules`
//! and run in the services before any write. Implementations:
//! - [`remote::SupabaseBackend`] — the hosted PostgREST backend, the only real store.
//! - `memory::MemoryBackend` — in-memory, for tests only (`test-support` feature).
//!
//! Balance/report math lives in [`ledger`] as pure functions over
//! [`Account`] + [`Entry`] rows.

pub mod ledger;
#[cfg(any(test, feature = "test-support"))]
pub mod memory;
pub mod remote;

use crate::error::{AppError, Result};
use crate::models::{
    Account, AccountBalance, Budget, Concept, Config, Entry, EntryFilter, EntryUpdate, NewAccount,
    NewEntry,
};

/// Builds the production backend: Supabase is the only store.
///
/// 1. No URL or no publishable key → [`AppError::NotConfigured`], without touching
///    the network or creating any file.
/// 2. One `ledger_status()` round trip, which also proves the session is valid.
pub fn connect(settings: &crate::settings::Settings) -> Result<Box<dyn LedgerBackend>> {
    let (url, key) = match (&settings.supabase_url, &settings.supabase_publishable_key) {
        (Some(u), Some(k)) if !u.trim().is_empty() && !k.trim().is_empty() => (u, k),
        _ => {
            return Err(AppError::NotConfigured(format!(
                "Supabase no está configurado. Corre:\n  \
                 money-tracker db remote login --url https://<ref>.supabase.co --key sb_publishable_…\n\
                 (o edita {}). Ver README, sección 1.",
                crate::settings::config_path().display()
            )))
        }
    };
    let be = remote::SupabaseBackend::new(url, key);
    be.status()?;
    Ok(Box::new(be))
}

/// Banker's-rounding-free "cents" rounding used everywhere balances are
/// derived (mirrors Postgres `round(n, 2)`).
/// f64 is fine at personal-finance scale; values stay well below 2^53.
pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
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
    // Ledger metadata and export.
    // ------------------------------------------------------------------
    /// Revision + schema version (`ledger_status()` RPC).
    fn status(&self) -> Result<crate::models::LedgerStatus>;
    /// The whole ledger at one point in time (`export_ledger()` RPC).
    fn export_snapshot(&self) -> Result<crate::models::LedgerSnapshot>;
    /// Email of the signed-in user, to prefill a backup's restore user.
    fn session_email(&self) -> Option<String>;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn connect_without_url_or_key_is_not_configured_and_offline() {
        let none = Settings::default();
        assert!(matches!(connect(&none), Err(AppError::NotConfigured(_))));
        let url_only = Settings { supabase_url: Some("https://x.supabase.co".into()), ..Default::default() };
        assert!(matches!(connect(&url_only), Err(AppError::NotConfigured(_))));
        let blank_key = Settings { supabase_publishable_key: Some("  ".into()), ..url_only };
        assert!(matches!(connect(&blank_key), Err(AppError::NotConfigured(_))));
    }
}
