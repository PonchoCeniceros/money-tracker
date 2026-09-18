//! Production wiring: a hosted [`LedgerBackend`] on top, a local SQLite
//! mirror below.
//!
//! [`MirroringBackend`] implements "always online" (FR-008): every read and
//! write goes to the remote; after each successful write it best-effort
//! refreshes the mirror from the remote and stashes any failure as a
//! warning (surfaced via `take_sync_warning`) rather than failing the user's
//! operation. A background poll keeps the mirror converged even while idle,
//! and `migrate_local_to_remote` moves a v1 SQLite database into Supabase
//! (remapping account ids, which is the one thing the two schemas differ
//! on).

use std::sync::Mutex;

use crate::error::{AppError, Result};
use crate::models::{Account, Budget, Config, Concept, Entry, EntryFilter, EntryUpdate, NewAccount, NewEntry};
use crate::storage::{LedgerBackend, LedgerDelta};

pub struct MirroringBackend {
    remote: Box<dyn LedgerBackend>,
    mirror: Box<dyn LedgerBackend>,
    warning: Mutex<Option<String>>,
}

impl MirroringBackend {
    pub fn new(remote: Box<dyn LedgerBackend>, mirror: Box<dyn LedgerBackend>) -> Self {
        MirroringBackend {
            remote,
            mirror,
            warning: Mutex::new(None),
        }
    }

    /// Refresh the mirror from the remote (best-effort). Any failure is
    /// recorded as a warning, never surfaced as an error.
    pub fn refresh_mirror(&self) {
        match self.do_refresh() {
            Ok(()) => {}
            Err(err) => {
                *self.warning.lock().unwrap() = Some(format!("Mirror behind: {err}"));
            }
        }
    }

    fn do_refresh(&self) -> Result<()> {
        let cursor = self.mirror.sync_cursor()?;
        let delta = self.remote.pull_changes_since(cursor)?;
        if !delta.is_empty() {
            self.mirror.apply_remote_snapshot(&delta)?;
        }
        Ok(())
    }

    /// Poll once: pull remote changes into the mirror (cheap when nothing
    /// changed). Called by the GUI's `useSync` timer and `sync poll`.
    pub fn poll(&self) -> Result<()> {
        self.do_refresh()
    }

    pub fn take_warning(&self) -> Option<String> {
        self.warning.lock().unwrap().take()
    }

    pub fn remote(&self) -> &dyn LedgerBackend {
        self.remote.as_ref()
    }

    pub fn mirror(&self) -> &dyn LedgerBackend {
        self.mirror.as_ref()
    }
}

impl LedgerBackend for MirroringBackend {
    fn raw_accounts(&self, include_archived: bool) -> Result<Vec<Account>> {
        self.remote.raw_accounts(include_archived)
    }

    fn entries(&self, f: &EntryFilter) -> Result<Vec<Entry>> {
        self.remote.entries(f)
    }

    fn insert_account(&self, new: &NewAccount) -> Result<i64> {
        let id = self.remote.insert_account(new)?;
        self.refresh_mirror();
        Ok(id)
    }

    fn set_archived(&self, id: i64) -> Result<()> {
        self.remote.set_archived(id)?;
        self.refresh_mirror();
        Ok(())
    }

    fn push_entries(&self, entries: &[NewEntry]) -> Result<Vec<Entry>> {
        let created = self.remote.push_entries(entries)?;
        self.refresh_mirror();
        Ok(created)
    }

    fn update_entry(&self, id: i64, upd: &EntryUpdate) -> Result<Entry> {
        let updated = self.remote.update_entry(id, upd)?;
        self.refresh_mirror();
        Ok(updated)
    }

    // Deletes are replayed to the mirror immediately through its own
    // delete path (not via a poll), so the tombstone never has to age out.
    fn delete_entry(&self, id: i64) -> Result<()> {
        self.remote.delete_entry(id)?;
        // Mirror-side: only if it has the row (it always does, but the
        // mirror may have started empty behind an existing remote).
        let _ = self.mirror.delete_entry(id);
        self.refresh_mirror();
        Ok(())
    }

    fn get_entry(&self, id: i64) -> Result<Entry> {
        self.remote.get_entry(id)
    }

    fn get_config(&self, key: &str) -> Result<Option<String>> {
        self.remote.get_config(key)
    }

    fn set_config(&self, key: &str, value: &str) -> Result<()> {
        self.remote.set_config(key, value)?;
        self.refresh_mirror();
        Ok(())
    }

    fn list_config(&self) -> Result<Vec<Config>> {
        self.remote.list_config()
    }

    fn list_concepts(&self, type_filter: Option<&str>) -> Result<Vec<Concept>> {
        self.remote.list_concepts(type_filter)
    }

    fn add_concept(&self, name: &str, concept_type: &str) -> Result<()> {
        self.remote.add_concept(name, concept_type)?;
        self.refresh_mirror();
        Ok(())
    }

    fn set_budget(&self, concept: &str, limit: f64, period: &str) -> Result<()> {
        self.remote.set_budget(concept, limit, period)?;
        self.refresh_mirror();
        Ok(())
    }

    fn list_budgets(&self, period: Option<&str>) -> Result<Vec<Budget>> {
        self.remote.list_budgets(period)
    }

    fn delete_budget(&self, concept: &str, period: &str) -> Result<()> {
        self.remote.delete_budget(concept, period)?;
        let _ = self.mirror.delete_budget(concept, period);
        self.refresh_mirror();
        Ok(())
    }

    fn remote_revision(&self) -> Result<i64> {
        self.remote.remote_revision()
    }

    fn sync_cursor(&self) -> Result<i64> {
        self.mirror.sync_cursor()
    }

    fn pull_changes_since(&self, cursor: i64) -> Result<LedgerDelta> {
        self.remote.pull_changes_since(cursor)
    }

    fn apply_remote_snapshot(&self, delta: &LedgerDelta) -> Result<()> {
        self.mirror.apply_remote_snapshot(delta)
    }

    fn take_sync_warning(&self) -> Option<String> {
        self.take_warning()
    }
}

/// Build the production backend: remote (Supabase) when configured, plain
/// local SQLite otherwise. In remote mode, `mirror_path` is the local
/// mirror; offline fallback means no config → SqliteBackend behaves exactly
/// like the pre-Supabase app.
pub fn production_backend(settings: &crate::settings::Settings) -> Result<Box<dyn LedgerBackend>> {
    if !settings.remote_configured() {
        return Ok(Box::new(crate::storage::sqlite::SqliteBackend::open_at(
            &crate::settings::mirror_path(),
        )?));
    }
    if !settings.is_complete() {
        return Err(AppError::Config(
            "MONEY_TRACKER_SUPABASE_URL is set but no publishable key is configured \
             (MONEY_TRACKER_SUPABASE_KEY or config.toml). Set the key to use remote mode."
                .into(),
        ));
    }
    let url = settings.supabase_url.as_deref().unwrap_or_default();
    let key = settings.supabase_publishable_key.as_deref().unwrap_or_default();
    let remote: Box<dyn LedgerBackend> =
        Box::new(crate::storage::remote::SupabaseBackend::new(url, key));
    let mirror = Box::new(crate::storage::sqlite::SqliteBackend::open_at(
        &crate::settings::mirror_path(),
    )?);
    Ok(Box::new(MirroringBackend::new(remote, mirror)))
}

/// Move a local SQLite database into Supabase. Account ids are the one thing
/// the two schemas don't share, so entries get their from/to ids remapped in
/// the same order the accounts were inserted. Uses the remote's own
/// `apply_entries` (server-side shape/overdraft checks) for the ledger rows
/// and plain inserts for the auxiliary tables.
pub fn migrate_local_to_remote(
    local: &dyn LedgerBackend,
    remote: &dyn LedgerBackend,
) -> Result<MigrateSummary> {
    eprintln!("[migrate] Starting concepts...");
    let concepts = local.list_concepts(None)?;
    for c in &concepts {
        if let Err(e) = remote.add_concept(&c.name, &c.concept_type) {
            if !matches!(e, AppError::Invalid(_)) && !format!("{e}").contains("duplicate") {
                return Err(e);
            }
        }
    }
    eprintln!("[migrate] Concepts done: {}", concepts.len());

    let accounts = local.raw_accounts(true)?;
    let mut id_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for a in &accounts {
        eprintln!("[migrate] Processing account: {} ({:?})", a.name, a.kind);
        let remote_account = remote.find_account_by_name(&a.name)?;
        let id = match remote_account {
            Some(existing) => {
                eprintln!("[migrate]   Found existing remote account id={}", existing.id);
                if a.archived && !existing.archived {
                    remote.set_archived(existing.id)?;
                }
                existing.id
            }
            None => {
                let new_account = NewAccount {
                    name: a.name.clone(),
                    kind: a.kind,
                    target_amount: a.target_amount,
                    credit_limit: a.credit_limit,
                    liquid: a.liquid,
                };
                let id = remote.insert_account(&new_account)?;
                eprintln!("[migrate]   Inserted new remote account id={}", id);
                if a.archived {
                    remote.set_archived(id)?;
                }
                id
            }
        };
        id_map.insert(a.id, id);
    }
    eprintln!("[migrate] Accounts done: {}", accounts.len());

    let mut entries = local.entries(&EntryFilter::default())?;
    // Sort chronologically (oldest first) so deposits/transfers arrive before
    // withdrawals that depend on them — RPC overdraft check sees prior inserts in same txn.
    entries.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
    eprintln!("[migrate] Pushing {} entries (sorted by date)...", entries.len());
    let mut batch = Vec::new();
    for e in &entries {
        let to = e.to_account_id.map(|id| *id_map.get(&id).unwrap_or(&0));
        let from = e.from_account_id.map(|id| *id_map.get(&id).unwrap_or(&0));
        if e.to_account_id.is_some() && to == Some(0) {
            return Err(AppError::Invalid(format!("account mapping lost for entry #{}", e.id)));
        }
        if e.from_account_id.is_some() && from == Some(0) {
            return Err(AppError::Invalid(format!("account mapping lost for entry #{}", e.id)));
        }
        batch.push(NewEntry {
            date: e.date.clone(),
            kind: e.kind,
            amount: e.amount,
            from_account_id: from,
            to_account_id: to,
            concept: e.concept.clone(),
            subconcept: e.subconcept.clone(),
            description: e.description.clone(),
        });
    }
    for (chunk_idx, chunk) in batch.chunks(50).enumerate() {
        eprintln!("[migrate] Pushing chunk {} ({} entries)...", chunk_idx, chunk.len());
        remote.push_entries(chunk)?;
        eprintln!("[migrate] Chunk {} done", chunk_idx);
    }

    let budgets = local.list_budgets(None)?;
    for b in &budgets {
        remote.set_budget(&b.concept, b.monthly_limit, &b.period)?;
    }

    let config = local.list_config()?;
    for c in &config {
        remote.set_config(&c.key, &c.value)?;
    }

    Ok(MigrateSummary {
        accounts: accounts.len(),
        entries: entries.len(),
        budgets: budgets.len(),
        concepts: concepts.len(),
    })
}

#[derive(Debug, Clone)]
pub struct MigrateSummary {
    pub accounts: usize,
    pub entries: usize,
    pub budgets: usize,
    pub concepts: usize,
}

impl LedgerDelta {
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
            && self.entries.is_empty()
            && self.budgets.is_empty()
            && self.concepts.is_empty()
            && self.config.is_empty()
            && self.deletes.is_empty()
    }
}