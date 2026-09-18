//! The local SQLite backend.
//!
//! Serves three roles:
//! 1. The in-memory store every `money_core` test runs against.
//! 2. The pre-Supabase standalone local database (same schema, same math).
//! 3. The mirror under `sync::MirroringBackend` — it can apply a
//!    [`LedgerDelta`], which carries the remote's own `rev`/`updated_at`.
//!
//! Writes that must be atomic (an expense + its overdraft check, a seed, an
//! emergency-split income) funnel through [`LedgerBackend::push_entries`],
//! which runs inside `BEGIN IMMEDIATE`.
//!
//! `rev`/`updated_at` stamping happens here in Rust, not in triggers: SQLite
//! trigger bodies can only run UPDATE/INSERT/DELETE/SELECT statements, and
//! `NEW` is read-only, so a trigger cannot rewrite an incoming row. Every
//! mutating statement below therefore stamps `rev`/`updated_at` explicitly
//! and bumps `sync_state.revision` in the same transaction
//! ([`begin_tx`]/[`stamp`]). `apply_remote_snapshot` bypasses the stamping so
//! the mirror keeps the cursor as its only convergence state.

use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};

use crate::db::CREATE_SCHEMA_SQL;
use crate::error::{AppError, Result};
use crate::models::{
    Account, AccountKind, Budget, Concept, Config, Entry, EntryFilter, EntryKind, NewAccount,
    NewEntry, EntryUpdate,
};
use crate::storage::{LedgerBackend, LedgerDelta};

pub struct SqliteBackend {
    conn: Mutex<Connection>,
}

/// Starts an immediate transaction for a local write. Statement-level writes
/// that don't already run in `push_entries`' own transaction use this so the
/// revision bump and the row write commit together.
fn begin_tx(conn: &Connection) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    Ok(())
}

fn commit_tx(conn: &Connection) -> Result<()> {
    conn.execute_batch("COMMIT")?;
    Ok(())
}

fn rollback_tx(conn: &Connection) {
    let _ = conn.execute_batch("ROLLBACK");
}

/// Bumps `sync_state.revision`; the next `rev` read from it belongs to the
/// current write. Must be called inside a transaction.
fn bump_revision(conn: &Connection) -> Result<()> {
    conn.execute("UPDATE sync_state SET revision = revision + 1 WHERE id = 1", [])?;
    Ok(())
}

/// The `rev`/`updated_at` pair to stamp on a local write, read from
/// `sync_state` after [`bump_revision`]. Both columns are display/sync
/// metadata; the mirror never consults them, so a NULL/missing row here
/// degrades to `0`/`NULL` (only reachable on hand-seeded test schemas).
impl SqliteBackend {
    /// Opens (and migrates, if needed) the SQLite file at `path`,
    /// initializing a schema if it's empty. Reuses `db::open_db_at` so the
    /// mirror, the standalone DB and the v1→Supabase migration all go
    /// through the exact same schema decision path.
    pub fn open_at(path: &std::path::Path) -> Result<Self> {
        Ok(Self {
            conn: Mutex::new(crate::db::open_db_at(path)?),
        })
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(CREATE_SCHEMA_SQL)?;
        seed_initial_data(&conn)?;
        conn.pragma_update(None, "user_version", crate::db::SCHEMA_VERSION)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}

fn seed_initial_data(conn: &Connection) -> Result<()> {
    for (name, ctype) in crate::db::SEED_CONCEPTS {
        conn.execute(
            "INSERT OR IGNORE INTO concepts (name, concept_type) VALUES (?1, ?2)",
            rusqlite::params![name, ctype],
        )?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO config (key, value) VALUES ('emergency_pct', '10')",
        [],
    )?;
    Ok(())
}

fn parse_kind(s: &str) -> AccountKind {
    match AccountKind::from_str(s) {
        Ok(k) => k,
        Err(_) => AccountKind::Spending,
    }
}

fn row_to_account(row: &rusqlite::Row) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: parse_kind(&row.get::<_, String>(2)?),
        target_amount: row.get(3)?,
        credit_limit: row.get(4)?,
        liquid: row.get::<_, i64>(5)? == 1,
        archived: row.get::<_, i64>(6)? == 1,
    })
}

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        date: row.get(1)?,
        kind: EntryKind::from_str(&row.get::<_, String>(2)?).unwrap_or(EntryKind::Expense),
        amount: row.get(3)?,
        from_account_id: row.get(4)?,
        to_account_id: row.get(5)?,
        from_account: row.get(6)?,
        to_account: row.get(7)?,
        concept: row.get(8)?,
        subconcept: row.get(9)?,
        description: row.get(10)?,
    })
}

const ENTRY_COLUMNS: &str = "id, date, kind, amount, from_account_id, to_account_id, \
     from_account, to_account, concept, subconcept, description";

fn entries_sql(f: &EntryFilter) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut sql = String::from("SELECT ") + ENTRY_COLUMNS + " FROM entries_view WHERE 1=1";
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(period) = &f.period {
        sql.push_str(" AND date >= ? AND date < ?");
        params.push(Box::new(period.start()));
        params.push(Box::new(period.end_exclusive()));
    }
    if let Some(up_to) = &f.up_to_date {
        sql.push_str(" AND date <= ?");
        params.push(Box::new(up_to.clone()));
    }
    if let Some(kind) = f.kind {
        sql.push_str(" AND kind = ?");
        params.push(Box::new(kind.as_str().to_string()));
    }
    if let Some(concept) = &f.concept {
        sql.push_str(" AND concept = ?");
        params.push(Box::new(concept.clone()));
    }
    if let Some(account_id) = f.account_id {
        sql.push_str(" AND (from_account_id = ? OR to_account_id = ?)");
        params.push(Box::new(account_id));
        params.push(Box::new(account_id));
    }
    sql.push_str(" ORDER BY date DESC, id DESC");
    if let Some(limit) = f.limit {
        sql.push_str(&format!(" LIMIT {limit}"));
    }
    (sql, params)
}

fn query_entries(conn: &Connection, f: &EntryFilter) -> Result<Vec<Entry>> {
    let (sql, params) = entries_sql(f);
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), row_to_entry)?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

fn fetch_entry_by_id(conn: &Connection, id: i64) -> Result<Entry> {
    conn.query_row(
        &(String::from("SELECT ") + ENTRY_COLUMNS + " FROM entries_view WHERE id = ?1"),
        rusqlite::params![id],
        row_to_entry,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("Entry #{id} not found")))
}

/// Overdraft policy shared by `push_entries`: target/emergency hard-error on
/// exceeding balance, credit hard-errors past the limit, spending never
/// blocks. Must be called inside the same transaction as the insert.
fn check_source_locked(conn: &Connection, account_id: i64, amount: f64) -> Result<()> {
    let (kind_str, balance, credit_limit): (String, f64, Option<f64>) = conn.query_row(
        "SELECT kind, balance, credit_limit FROM account_balances WHERE id = ?1",
        rusqlite::params![account_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let kind = AccountKind::from_str(&kind_str)?;
    match kind {
        AccountKind::Target | AccountKind::Emergency => {
            if amount > balance {
                return Err(AppError::Invalid(format!(
                    "Insufficient balance: have ${balance:.2}, need ${amount:.2}"
                )));
            }
        }
        AccountKind::Credit => {
            if let Some(limit) = credit_limit {
                let debt = (-balance).max(0.0);
                if debt + amount > limit {
                    let available = limit - debt;
                    return Err(AppError::Invalid(format!(
                        "Exceeds credit limit: ${available:.2} available, ${amount:.2} requested"
                    )));
                }
            }
        }
        AccountKind::Spending => {}
    }
    Ok(())
}

impl LedgerBackend for SqliteBackend {
    fn raw_accounts(&self, include_archived: bool) -> Result<Vec<Account>> {
        let conn = self.connection();
        let sql = if include_archived {
            "SELECT id, name, kind, target_amount, credit_limit, liquid, archived
               FROM accounts ORDER BY kind, name"
        } else {
            "SELECT id, name, kind, target_amount, credit_limit, liquid, archived
               FROM accounts WHERE archived = 0 ORDER BY kind, name"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], row_to_account)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    fn entries(&self, f: &EntryFilter) -> Result<Vec<Entry>> {
        query_entries(&self.connection(), f)
    }

    fn insert_account(&self, new: &NewAccount) -> Result<i64> {
        let conn = self.connection();
        begin_tx(&conn)?;
        let result = (|| -> Result<i64> {
            bump_revision(&conn)?;
            conn.execute(
                "INSERT INTO accounts (name, kind, target_amount, credit_limit, liquid, updated_at, rev)
                 VALUES (?1, ?2, ?3, ?4, ?5,
                         strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                         (SELECT revision FROM sync_state WHERE id = 1))",
                rusqlite::params![
                    new.name,
                    new.kind.as_str(),
                    new.target_amount,
                    new.credit_limit,
                    new.liquid as i64,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })();
        match result {
            Ok(id) => {
                commit_tx(&conn)?;
                Ok(id)
            }
            Err(err) => {
                rollback_tx(&conn);
                Err(err)
            }
        }
    }

    fn set_archived(&self, id: i64) -> Result<()> {
        let conn = self.connection();
        begin_tx(&conn)?;
        let result = (|| -> Result<()> {
            bump_revision(&conn)?;
            conn.execute(
                "UPDATE accounts
                    SET archived = 1,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        rev = (SELECT revision FROM sync_state WHERE id = 1)
                  WHERE id = ?1",
                rusqlite::params![id],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                commit_tx(&conn)?;
                Ok(())
            }
            Err(err) => {
                rollback_tx(&conn);
                Err(err)
            }
        }
    }

    fn push_entries(&self, entries: &[NewEntry]) -> Result<Vec<Entry>> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.connection();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<Vec<i64>> {
            let mut ids = Vec::new();
            for e in entries {
                match e.kind {
                    EntryKind::Expense | EntryKind::Transfer => {
                        let from = e.from_account_id.ok_or_else(|| {
                            AppError::Invalid("Entry must have a source account".into())
                        })?;
                        check_source_locked(&conn, from, e.amount)?;
                    }
                    EntryKind::Income | EntryKind::Opening => {}
                }
                bump_revision(&conn)?;
                conn.execute(
                    "INSERT INTO entries (date, kind, amount, from_account_id, to_account_id,
                                          concept, subconcept, description, updated_at, rev)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                             strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                             (SELECT revision FROM sync_state WHERE id = 1))",
                    rusqlite::params![
                        e.date, e.kind.as_str(), e.amount, e.from_account_id, e.to_account_id,
                        e.concept, e.subconcept, e.description,
                    ],
                )?;
                ids.push(conn.last_insert_rowid());
            }
            Ok(ids)
        })();
        match result {
            Ok(ids) => {
                conn.execute_batch("COMMIT")?;
                // `conn` is still alive here so we must not call `self.entries`
                // (re-entrant lock). Read the fresh rows off the same handle.
                let mut out = Vec::with_capacity(ids.len());
                for id in ids {
                    out.push(fetch_entry_by_id(&conn, id)?);
                }
                Ok(out)
            }
            Err(err) => {
                conn.execute_batch("ROLLBACK")?;
                Err(err)
            }
        }
    }

    fn update_entry(&self, id: i64, upd: &EntryUpdate) -> Result<Entry> {
        let current = self.get_entry(id)?;

        match current.kind {
            EntryKind::Income | EntryKind::Opening => {
                if upd.from_account_id.is_some() {
                    return Err(AppError::Invalid(
                        "This entry has no source account to change (it's an income/opening entry)"
                            .into(),
                    ));
                }
            }
            EntryKind::Expense => {
                if upd.to_account_id.is_some() {
                    return Err(AppError::Invalid(
                        "This entry has no destination account to change (it's an expense)".into(),
                    ));
                }
            }
            EntryKind::Transfer => {}
        }
        if matches!(current.kind, EntryKind::Transfer | EntryKind::Opening)
            && upd.concept.is_some()
        {
            return Err(AppError::Invalid(
                "Transfers and opening balances don't carry a concept".into(),
            ));
        }

        let new_date = upd.date.clone().unwrap_or_else(|| current.date.clone());
        let new_amount = upd.amount.unwrap_or(current.amount);
        let new_from = upd.from_account_id.or(current.from_account_id);
        let new_to = upd.to_account_id.or(current.to_account_id);
        let new_concept = upd.concept.clone().or_else(|| current.concept.clone());
        let new_subconcept = upd
            .subconcept
            .clone()
            .or_else(|| current.subconcept.clone());
        let new_description = upd
            .description
            .clone()
            .or_else(|| current.description.clone());

        crate::period::validate_date(&new_date)?;
        if new_amount <= 0.0 {
            return Err(AppError::Invalid("Amount must be positive".into()));
        }
        if current.kind == EntryKind::Transfer && new_from == new_to {
            return Err(AppError::Invalid(
                "Transfer source and destination cannot be the same account".into(),
            ));
        }
        if matches!(current.kind, EntryKind::Income | EntryKind::Expense)
            && new_concept.is_none()
        {
            return Err(AppError::Invalid("Concept is required".into()));
        }

        let conn = self.connection();
        begin_tx(&conn)?;
        let result = (|| -> Result<()> {
            bump_revision(&conn)?;
            conn.execute(
                "UPDATE entries
                    SET date = ?1, amount = ?2, from_account_id = ?3, to_account_id = ?4,
                        concept = ?5, subconcept = ?6, description = ?7,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        rev = (SELECT revision FROM sync_state WHERE id = 1)
                  WHERE id = ?8",
                rusqlite::params![
                    new_date, new_amount, new_from, new_to, new_concept, new_subconcept,
                    new_description, id,
                ],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                commit_tx(&conn)?;
                // `conn` (the lock) is still alive, so read back off the same
                // handle instead of re-entering `self.connection()`.
                fetch_entry_by_id(&conn, id)
            }
            Err(err) => {
                rollback_tx(&conn);
                Err(err)
            }
        }
    }

    fn delete_entry(&self, id: i64) -> Result<()> {
        let conn = self.connection();
        begin_tx(&conn)?;
        let result = (|| -> Result<usize> {
            bump_revision(&conn)?;
            conn.execute("DELETE FROM entries WHERE id = ?1", rusqlite::params![id])
                .map_err(AppError::from)
        })();
        match result {
            Ok(affected) => {
                commit_tx(&conn)?;
                if affected == 0 {
                    return Err(AppError::NotFound(format!("Entry #{id} not found")));
                }
                Ok(())
            }
            Err(err) => {
                rollback_tx(&conn);
                Err(err)
            }
        }
    }

    fn get_entry(&self, id: i64) -> Result<Entry> {
        fetch_entry_by_id(&self.connection(), id)
    }

    fn get_config(&self, key: &str) -> Result<Option<String>> {
        self.connection()
            .query_row(
                "SELECT value FROM config WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(AppError::from)
    }

    fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.connection();
        begin_tx(&conn)?;
        let result = (|| -> Result<()> {
            bump_revision(&conn)?;
            conn.execute(
                "INSERT OR REPLACE INTO config (key, value, updated_at, rev)
                 VALUES (?1, ?2,
                         strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                         (SELECT revision FROM sync_state WHERE id = 1))",
                rusqlite::params![key, value],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                commit_tx(&conn)?;
                Ok(())
            }
            Err(err) => {
                rollback_tx(&conn);
                Err(err)
            }
        }
    }

    fn list_config(&self) -> Result<Vec<Config>> {
        let conn = self.connection();
        let mut stmt = conn.prepare("SELECT key, value FROM config ORDER BY key")?;
        let rows = stmt.query_map([], |row| {
            Ok(Config {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    fn list_concepts(&self, type_filter: Option<&str>) -> Result<Vec<Concept>> {
        let conn = self.connection();
        let mut result = Vec::new();
        match type_filter {
            Some(t) => {
                let mut stmt = conn.prepare(
                    "SELECT id, name, concept_type FROM concepts WHERE concept_type IN (?1, 'both') ORDER BY name",
                )?;
                let rows = stmt.query_map(rusqlite::params![t], |row| {
                    Ok(Concept {
                        id: Some(row.get(0)?),
                        name: row.get(1)?,
                        concept_type: row.get(2)?,
                    })
                })?;
                for row in rows {
                    result.push(row?);
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, name, concept_type FROM concepts ORDER BY concept_type, name",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(Concept {
                        id: Some(row.get(0)?),
                        name: row.get(1)?,
                        concept_type: row.get(2)?,
                    })
                })?;
                for row in rows {
                    result.push(row?);
                }
            }
        }
        Ok(result)
    }

    fn add_concept(&self, name: &str, concept_type: &str) -> Result<()> {
        if !["expense", "income", "both"].contains(&concept_type) {
            return Err(AppError::Invalid(
                "Type must be expense, income, or both".into(),
            ));
        }
        let conn = self.connection();
        begin_tx(&conn)?;
        let result = (|| -> Result<()> {
            bump_revision(&conn)?;
            conn.execute(
                "INSERT INTO concepts (name, concept_type, updated_at, rev)
                 VALUES (?1, ?2,
                         strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                         (SELECT revision FROM sync_state WHERE id = 1))",
                rusqlite::params![name, concept_type],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                commit_tx(&conn)?;
                Ok(())
            }
            Err(err) => {
                rollback_tx(&conn);
                Err(err)
            }
        }
    }

    fn set_budget(&self, concept: &str, limit: f64, period: &str) -> Result<()> {
        if limit <= 0.0 {
            return Err(AppError::Invalid("Limit must be positive".into()));
        }
        let conn = self.connection();
        begin_tx(&conn)?;
        let result = (|| -> Result<()> {
            bump_revision(&conn)?;
            conn.execute(
                "INSERT INTO budgets (concept, monthly_limit, period, updated_at, rev)
                 VALUES (?1, ?2, ?3,
                         strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                         (SELECT revision FROM sync_state WHERE id = 1))
                 ON CONFLICT(concept, period) DO UPDATE SET
                    monthly_limit = excluded.monthly_limit,
                    updated_at = excluded.updated_at,
                    rev = excluded.rev",
                rusqlite::params![concept, limit, period],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                commit_tx(&conn)?;
                Ok(())
            }
            Err(err) => {
                rollback_tx(&conn);
                Err(err)
            }
        }
    }

    fn list_budgets(&self, period: Option<&str>) -> Result<Vec<Budget>> {
        let conn = self.connection();
        let mut result = Vec::new();
        let mut stmt = match period {
            Some(_) => conn.prepare(
                "SELECT id, concept, monthly_limit, period FROM budgets WHERE period = ?1 ORDER BY concept",
            )?,
            None => conn.prepare(
                "SELECT id, concept, monthly_limit, period FROM budgets ORDER BY concept, period",
            )?,
        };
        let rows = match period {
            Some(p) => stmt.query_map(rusqlite::params![p], row_to_budget)?,
            None => stmt.query_map([], row_to_budget)?,
        };
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    fn delete_budget(&self, concept: &str, period: &str) -> Result<()> {
        let conn = self.connection();
        begin_tx(&conn)?;
        let result = (|| -> Result<()> {
            bump_revision(&conn)?;
            conn.execute(
                "DELETE FROM budgets WHERE concept = ?1 AND period = ?2",
                rusqlite::params![concept, period],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                commit_tx(&conn)?;
                Ok(())
            }
            Err(err) => {
                rollback_tx(&conn);
                Err(err)
            }
        }
    }

    fn sync_cursor(&self) -> Result<i64> {
        let conn = self.connection();
        Ok(conn.query_row(
            "SELECT mirror_revision FROM sync_state WHERE id = 1",
            [],
            |row| row.get(0),
        )?)
    }

    fn reset_mirror_cursor(&self) -> Result<()> {
        self.connection().execute(
            "UPDATE sync_state SET mirror_revision = 0 WHERE id = 1",
            [],
        )?;
        Ok(())
    }

    fn apply_remote_snapshot(&self, delta: &LedgerDelta) -> Result<()> {
        let conn = self.connection();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            // The upserts below key on `id`, but `concepts.name`, `accounts.name`
            // and `budgets(concept, period)` are UNIQUE too: a remote row can
            // collide with a *different* local row on the name while its own id
            // matches yet another. SQLite's upsert resolves one conflict only, so
            // the second one aborts the whole poll ("UNIQUE constraint failed:
            // concepts.name"). A fresh mirror hits this immediately, because
            // `db::open_db_at` seeds SEED_CONCEPTS with local ids that have
            // nothing to do with the remote's. Clear the loser by name first —
            // the winning row is re-inserted right after, and `defer_foreign_keys`
            // keeps `entries.concept -> concepts(name)` from tripping mid-batch.
            conn.execute_batch("PRAGMA defer_foreign_keys = ON")?;
            for c in &delta.concepts {
                conn.execute(
                    "DELETE FROM concepts WHERE name = ?1 AND id <> ?2",
                    rusqlite::params![c.name, c.id],
                )?;
                conn.execute(
                    "INSERT INTO concepts (id, name, concept_type, user_id, updated_at, rev)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT DO UPDATE SET
                        id = excluded.id, name = excluded.name,
                        concept_type = excluded.concept_type,
                        user_id = excluded.user_id, updated_at = excluded.updated_at,
                        rev = excluded.rev",
                    rusqlite::params![c.id, c.name, c.concept_type, Option::<String>::None, Option::<String>::None, 0],
                )?;
            }
            for a in &delta.accounts {
                conn.execute(
                    "DELETE FROM accounts WHERE name = ?1 AND id <> ?2",
                    rusqlite::params![a.name, a.id],
                )?;
                // `idx_accounts_one_emergency` is a second unique key the
                // upsert can't see: an incoming active emergency account must
                // evict any *other* active emergency row first.
                if a.kind == AccountKind::Emergency && !a.archived {
                    conn.execute(
                        "DELETE FROM accounts
                         WHERE kind = 'emergency' AND archived = 0 AND id <> ?1",
                        rusqlite::params![a.id],
                    )?;
                }
                conn.execute(
                    "INSERT INTO accounts (id, name, kind, target_amount, credit_limit, liquid, archived, user_id, updated_at, rev)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT DO UPDATE SET
                        id = excluded.id, name = excluded.name, kind = excluded.kind,
                        target_amount = excluded.target_amount, credit_limit = excluded.credit_limit,
                        liquid = excluded.liquid, archived = excluded.archived,
                        user_id = excluded.user_id, updated_at = excluded.updated_at,
                        rev = excluded.rev",
                    rusqlite::params![
                        a.id, a.name, a.kind.as_str(), a.target_amount, a.credit_limit,
                        a.liquid as i64, a.archived as i64,
                        Option::<String>::None, Option::<String>::None, 0,
                    ],
                )?;
            }
            // Note: remote `updated_at`/`rev` are *not* carried into the
            // mirror's raw columns here deliberately — the remote's timestamps
            // are timestamptz, ours are a local format, and the cursor is the
            // only piece the mirror needs to stay converged. Storing them
            // would invite format-drift bugs for zero consumers.
            for b in &delta.budgets {
                conn.execute(
                    "DELETE FROM budgets WHERE concept = ?1 AND period = ?2 AND id <> ?3",
                    rusqlite::params![b.concept, b.period, b.id],
                )?;
                conn.execute(
                    "INSERT INTO budgets (id, concept, monthly_limit, period, user_id, updated_at, rev)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT DO UPDATE SET
                        id = excluded.id, concept = excluded.concept,
                        monthly_limit = excluded.monthly_limit,
                        period = excluded.period, user_id = excluded.user_id,
                        updated_at = excluded.updated_at, rev = excluded.rev",
                    rusqlite::params![b.id, b.concept, b.monthly_limit, b.period, Option::<String>::None, Option::<String>::None, 0],
                )?;
            }

            for e in &delta.entries {
                conn.execute(
                    "INSERT INTO entries (id, date, kind, amount, from_account_id, to_account_id,
                                          concept, subconcept, description, user_id, updated_at, rev)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                     ON CONFLICT(id) DO UPDATE SET
                        date = excluded.date, kind = excluded.kind, amount = excluded.amount,
                        from_account_id = excluded.from_account_id, to_account_id = excluded.to_account_id,
                        concept = excluded.concept, subconcept = excluded.subconcept,
                        description = excluded.description, user_id = excluded.user_id,
                        updated_at = excluded.updated_at, rev = excluded.rev",
                    rusqlite::params![
                        e.id, e.date, e.kind.as_str(), e.amount, e.from_account_id, e.to_account_id,
                        e.concept, e.subconcept, e.description,
                        Option::<String>::None, Option::<String>::None, 0,
                    ],
                )?;
            }
            for c in &delta.config {
                conn.execute(
                    "INSERT INTO config (key, value, user_id, updated_at, rev)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(key) DO UPDATE SET
                        value = excluded.value, user_id = excluded.user_id,
                        updated_at = excluded.updated_at, rev = excluded.rev",
                    rusqlite::params![c.key, c.value, Option::<String>::None, Option::<String>::None, 0],
                )?;
            }

            // Replay deletions recorded by remote AFTER DELETE triggers.
            // `table` comes from the remote's own `tg_table_name` (one of
            // exactly these five), so whitelisting is anti-injection, not a
            // features decision.
            for (table, row_id) in &delta.deletes {
                let table = match table.as_str() {
                    "entries" | "budgets" | "accounts" | "concepts" | "config" => table.as_str(),
                    other => {
                        return Err(AppError::Remote(format!(
                            "unknown table in delete tombstone: {other}"
                        )));
                    }
                };
                conn.execute(
                    &format!("DELETE FROM {table} WHERE id = ?1"),
                    rusqlite::params![row_id],
                )?;
            }

            conn.execute(
                "UPDATE sync_state SET mirror_revision = ?1 WHERE id = 1",
                rusqlite::params![delta.cursor],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(err) => {
                conn.execute_batch("ROLLBACK")?;
                Err(err)
            }
        }
    }
}

fn row_to_budget(row: &rusqlite::Row) -> rusqlite::Result<Budget> {
    Ok(Budget {
        id: Some(row.get(0)?),
        concept: row.get(1)?,
        monthly_limit: row.get(2)?,
        period: row.get(3)?,
    })
}

impl std::fmt::Debug for SqliteBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SqliteBackend")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NewAccount;

    fn backend() -> SqliteBackend {
        SqliteBackend::open_memory().unwrap()
    }

    #[test]
    fn accounts_and_entries_roundtrip() {
        let b = backend();
        let id = b
            .insert_account(&NewAccount::spending("debito"))
            .unwrap();
        let e = NewEntry::income("2026-08-01", 1000.0, id, "Nomina").unwrap();
        let created = b.push_entries(&[e]).unwrap();
        assert_eq!(created[0].to_account_id, Some(id));
        assert_eq!(b.get_entry(created[0].id).unwrap().amount, 1000.0);
        let balances = b.list_accounts(false).unwrap();
        assert_eq!(balances[0].balance, 1000.0);
    }

    #[test]
    fn push_entries_is_atomic_on_overdraft() {
        let b = backend();
        let id = b
            .insert_account(&NewAccount::emergency("fondo"))
            .unwrap();
        let e = NewEntry::expense("2026-08-01", 500.0, id, "Servicios").unwrap();
        let err = b.push_entries(&[e]).unwrap_err();
        assert!(format!("{err}").contains("Insufficient balance"));
        let entries = b.entries(&EntryFilter::default()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn apply_remote_snapshot_respects_remote_rows() {
        let mirror = backend();
        let mut delta = LedgerDelta {
            cursor: 7,
            ..Default::default()
        };
        delta.accounts.push(Account {
            id: 1,
            name: "debito remoto".into(),
            kind: AccountKind::Spending,
            target_amount: None,
            credit_limit: None,
            liquid: true,
            archived: false,
        });
        delta.accounts.push(Account {
            id: 2,
            name: "fondo remoto".into(),
            kind: AccountKind::Emergency,
            target_amount: None,
            credit_limit: None,
            liquid: true,
            archived: false,
        });
        delta.entries.push(Entry {
            id: 10,
            date: "2026-08-01".into(),
            kind: EntryKind::Income,
            amount: 1000.0,
            from_account_id: None,
            to_account_id: Some(1),
            from_account: None,
            to_account: Some("debito remoto".into()),
            concept: Some("Nomina".into()),
            subconcept: None,
            description: None,
        });
        mirror.apply_remote_snapshot(&delta).unwrap();
        assert_eq!(mirror.sync_cursor().unwrap(), 7);
        let balances = mirror.list_accounts(false).unwrap();
        assert_eq!(balances.len(), 2);
        assert_eq!(
            balances.iter().find(|a| a.id == 1).unwrap().balance,
            1000.0
        );

        // Re-applying (new polls, cursor stays) must be idempotent.
        mirror.apply_remote_snapshot(&delta).unwrap();
        assert_eq!(mirror.list_accounts(false).unwrap().len(), 2);
        assert_eq!(mirror.sync_cursor().unwrap(), 7);
    }

    /// A fresh mirror is seeded with SEED_CONCEPTS under local ids, so the
    /// remote's first snapshot arrives with the same names under *different*
    /// ids — and with an id that a different local name already owns. The
    /// upsert can only resolve one conflict, so this used to abort the whole
    /// poll with "UNIQUE constraint failed: concepts.name".
    #[test]
    fn apply_remote_snapshot_survives_seeded_concept_id_shuffle() {
        let mirror = backend();
        let seeded = mirror.list_concepts(None).unwrap();
        assert!(seeded.len() >= 2, "mirror should arrive seeded");

        // Same names as the seed, ids rotated by one: every row collides on
        // `name` with one local row and on `id` with another.
        let concepts: Vec<Concept> = seeded
            .iter()
            .enumerate()
            .map(|(i, c)| Concept {
                id: seeded[(i + 1) % seeded.len()].id,
                name: c.name.clone(),
                concept_type: c.concept_type.clone(),
            })
            .collect();
        let delta = LedgerDelta {
            cursor: 16,
            concepts,
            ..Default::default()
        };

        mirror.apply_remote_snapshot(&delta).unwrap();
        assert_eq!(mirror.sync_cursor().unwrap(), 16);

        let after = mirror.list_concepts(None).unwrap();
        assert_eq!(after.len(), seeded.len(), "no concept lost or duplicated");
        for c in &delta.concepts {
            let row = after.iter().find(|r| r.name == c.name).unwrap();
            assert_eq!(row.id, c.id, "mirror must adopt the remote id");
        }
    }

    /// The remote renaming an account (or handing the same name a new id) must
    /// not deadlock the mirror on `accounts.name` / the one-emergency index.
    #[test]
    fn apply_remote_snapshot_survives_account_id_and_emergency_collisions() {
        let mirror = backend();
        mirror
            .insert_account(&NewAccount::spending("debito"))
            .unwrap();
        let local_emergency = mirror
            .insert_account(&NewAccount {
                name: "fondo local".into(),
                kind: AccountKind::Emergency,
                target_amount: None,
                credit_limit: None,
                liquid: true,
            })
            .unwrap();

        let delta = LedgerDelta {
            cursor: 3,
            accounts: vec![
                // Same name as the local row, different id.
                Account {
                    id: 900,
                    name: "debito".into(),
                    kind: AccountKind::Spending,
                    target_amount: None,
                    credit_limit: None,
                    liquid: true,
                    archived: false,
                },
                // A *differently named* active emergency: collides only on
                // `idx_accounts_one_emergency`, which the upsert can't see.
                Account {
                    id: 901,
                    name: "fondo remoto".into(),
                    kind: AccountKind::Emergency,
                    target_amount: None,
                    credit_limit: None,
                    liquid: true,
                    archived: false,
                },
            ],
            ..Default::default()
        };

        mirror.apply_remote_snapshot(&delta).unwrap();
        let after = mirror.list_accounts(true).unwrap();
        assert_eq!(after.len(), 2, "stale local rows evicted, not duplicated");
        assert!(after.iter().any(|a| a.id == 900 && a.name == "debito"));
        assert!(after.iter().any(|a| a.id == 901));
        assert!(
            !after.iter().any(|a| a.id == local_emergency),
            "the superseded local emergency account is gone"
        );
    }
}