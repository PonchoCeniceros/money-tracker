//! In-memory `LedgerBackend` for tests. Compiled only with `cfg(test)` or the
//! `test-support` feature.
//!
//! It imitates what a real store guarantees (ids, ordering, uniqueness,
//! foreign keys, atomic batches) and deliberately *none* of the accounting
//! rules: those live in `crate::rules` and run in the services before any
//! write, which is exactly what these tests are meant to exercise.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use crate::error::{AppError, Result};
use crate::models::{
    Account, Budget, Concept, Config, Entry, EntryFilter, EntryUpdate, LedgerSnapshot, LedgerStatus,
    NewAccount, NewEntry,
};
use crate::storage::LedgerBackend;

/// The concept vocabulary and `emergency_pct` a fresh ledger used to be seeded with.
pub const SEED_CONCEPTS: &[(&str, &str)] = &[
    ("Discrecional", "expense"),
    ("Transporte", "expense"),
    ("Servicios", "expense"),
    ("Alimentos", "expense"),
    ("Extraordinario", "expense"),
    ("Sandbox Inversión", "expense"),
    ("Nomina", "income"),
    ("Vales de Despensa", "income"),
    ("Ahorro Patronal", "income"),
    ("Extra", "income"),
];

#[derive(Default)]
struct State {
    accounts: Vec<Account>,
    entries: Vec<NewEntryRow>,
    concepts: Vec<Concept>,
    budgets: Vec<Budget>,
    config: BTreeMap<String, String>,
    next_account: i64,
    next_entry: i64,
    next_concept: i64,
    next_budget: i64,
    revision: i64,
}

#[derive(Clone)]
struct NewEntryRow {
    id: i64,
    e: NewEntry,
}

#[derive(Default)]
pub struct MemoryBackend {
    state: Mutex<State>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// A ledger with the base concepts and `emergency_pct = 10`.
    pub fn seeded() -> Self {
        let be = Self::new();
        {
            let mut s = be.lock();
            for (name, ctype) in SEED_CONCEPTS {
                s.next_concept += 1;
                let id = s.next_concept;
                s.concepts.push(Concept {
                    id: Some(id),
                    name: name.to_string(),
                    concept_type: ctype.to_string(),
                });
            }
            s.config.insert("emergency_pct".into(), "10".into());
        }
        be
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl State {
    fn account_name(&self, id: Option<i64>) -> Option<String> {
        id.and_then(|id| self.accounts.iter().find(|a| a.id == id).map(|a| a.name.clone()))
    }

    fn to_entry(&self, row: &NewEntryRow) -> Entry {
        Entry {
            id: row.id,
            date: row.e.date.clone(),
            kind: row.e.kind,
            amount: row.e.amount,
            from_account_id: row.e.from_account_id,
            to_account_id: row.e.to_account_id,
            from_account: self.account_name(row.e.from_account_id),
            to_account: self.account_name(row.e.to_account_id),
            concept: row.e.concept.clone(),
            subconcept: row.e.subconcept.clone(),
            description: row.e.description.clone(),
        }
    }

    /// Foreign keys a real store enforces: referenced accounts and concept must exist.
    fn check_refs(&self, from: Option<i64>, to: Option<i64>, concept: Option<&str>) -> Result<()> {
        for id in [from, to].into_iter().flatten() {
            if !self.accounts.iter().any(|a| a.id == id) {
                return Err(AppError::NotFound(format!("Account #{id} not found")));
            }
        }
        if let Some(c) = concept {
            self.check_concept(c)?;
        }
        Ok(())
    }

    fn check_concept(&self, concept: &str) -> Result<()> {
        if !self.concepts.iter().any(|c| c.name == concept) {
            return Err(AppError::Invalid(format!("Concept '{concept}' does not exist")));
        }
        Ok(())
    }

    fn bump(&mut self) {
        self.revision += 1;
    }
}

impl LedgerBackend for MemoryBackend {
    fn raw_accounts(&self, include_archived: bool) -> Result<Vec<Account>> {
        let s = self.lock();
        let mut out: Vec<Account> = s
            .accounts
            .iter()
            .filter(|a| include_archived || !a.archived)
            .cloned()
            .collect();
        out.sort_by(|a, b| (a.kind.as_str(), &a.name).cmp(&(b.kind.as_str(), &b.name)));
        Ok(out)
    }

    fn entries(&self, f: &EntryFilter) -> Result<Vec<Entry>> {
        let s = self.lock();
        let mut rows: Vec<&NewEntryRow> = s
            .entries
            .iter()
            .filter(|r| {
                let e = &r.e;
                f.period.as_ref().is_none_or(|p| {
                    e.date.as_str() >= p.start().as_str() && e.date.as_str() < p.end_exclusive().as_str()
                }) && f.up_to_date.as_ref().is_none_or(|d| e.date.as_str() <= d.as_str())
                    && f.kind.is_none_or(|k| e.kind == k)
                    && f.concept.as_ref().is_none_or(|c| e.concept.as_ref() == Some(c))
                    && f.account_id.is_none_or(|id| {
                        e.from_account_id == Some(id) || e.to_account_id == Some(id)
                    })
            })
            .collect();
        rows.sort_by(|a, b| (&b.e.date, b.id).cmp(&(&a.e.date, a.id)));
        if let Some(limit) = f.limit {
            rows.truncate(limit as usize);
        }
        Ok(rows.into_iter().map(|r| s.to_entry(r)).collect())
    }

    fn insert_account(&self, new: &NewAccount) -> Result<i64> {
        let mut s = self.lock();
        if s.accounts.iter().any(|a| a.name == new.name) {
            return Err(AppError::Invalid(format!("Account '{}' already exists", new.name)));
        }
        s.next_account += 1;
        let id = s.next_account;
        s.accounts.push(Account {
            id,
            name: new.name.clone(),
            kind: new.kind,
            target_amount: new.target_amount,
            credit_limit: new.credit_limit,
            liquid: new.liquid,
            archived: false,
        });
        s.bump();
        Ok(id)
    }

    fn set_archived(&self, id: i64) -> Result<()> {
        let mut s = self.lock();
        if let Some(a) = s.accounts.iter_mut().find(|a| a.id == id) {
            a.archived = true;
        }
        s.bump();
        Ok(())
    }

    fn push_entries(&self, entries: &[NewEntry]) -> Result<Vec<Entry>> {
        let mut s = self.lock();
        // Validate the whole batch before inserting anything: all-or-nothing.
        for e in entries {
            s.check_refs(e.from_account_id, e.to_account_id, e.concept.as_deref())?;
        }
        let mut out = Vec::with_capacity(entries.len());
        for e in entries {
            s.next_entry += 1;
            let row = NewEntryRow { id: s.next_entry, e: e.clone() };
            s.entries.push(row.clone());
            s.bump();
            out.push(s.to_entry(&row));
        }
        Ok(out)
    }

    fn update_entry(&self, id: i64, upd: &EntryUpdate) -> Result<Entry> {
        let mut s = self.lock();
        let idx = s
            .entries
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| AppError::NotFound(format!("Entry #{id} not found")))?;
        let current = s.entries[idx].e.clone();
        let from = upd.from_account_id.or(current.from_account_id);
        let to = upd.to_account_id.or(current.to_account_id);
        s.check_refs(from, to, upd.concept.as_deref())?;
        let row = &mut s.entries[idx].e;
        if let Some(d) = &upd.date {
            row.date = d.clone();
        }
        if let Some(a) = upd.amount {
            row.amount = a;
        }
        row.from_account_id = from;
        row.to_account_id = to;
        if let Some(c) = &upd.concept {
            row.concept = Some(c.clone());
        }
        if let Some(sc) = &upd.subconcept {
            row.subconcept = Some(sc.clone());
        }
        if let Some(d) = &upd.description {
            row.description = Some(d.clone());
        }
        s.bump();
        let row = s.entries[idx].clone();
        Ok(s.to_entry(&row))
    }

    fn delete_entry(&self, id: i64) -> Result<()> {
        let mut s = self.lock();
        let before = s.entries.len();
        s.entries.retain(|r| r.id != id);
        if s.entries.len() == before {
            return Err(AppError::NotFound(format!("Entry #{id} not found")));
        }
        s.bump();
        Ok(())
    }

    fn get_entry(&self, id: i64) -> Result<Entry> {
        let s = self.lock();
        s.entries
            .iter()
            .find(|r| r.id == id)
            .map(|r| s.to_entry(r))
            .ok_or_else(|| AppError::NotFound(format!("Entry #{id} not found")))
    }

    fn get_config(&self, key: &str) -> Result<Option<String>> {
        Ok(self.lock().config.get(key).cloned())
    }

    fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let mut s = self.lock();
        s.config.insert(key.to_string(), value.to_string());
        s.bump();
        Ok(())
    }

    fn list_config(&self) -> Result<Vec<Config>> {
        Ok(self
            .lock()
            .config
            .iter()
            .map(|(k, v)| Config { key: k.clone(), value: v.clone() })
            .collect())
    }

    fn list_concepts(&self, type_filter: Option<&str>) -> Result<Vec<Concept>> {
        let s = self.lock();
        let mut out: Vec<Concept> = match type_filter {
            Some(t) => s
                .concepts
                .iter()
                .filter(|c| c.concept_type == t || c.concept_type == "both")
                .cloned()
                .collect(),
            None => s.concepts.clone(),
        };
        match type_filter {
            Some(_) => out.sort_by(|a, b| a.name.cmp(&b.name)),
            None => out.sort_by(|a, b| (&a.concept_type, &a.name).cmp(&(&b.concept_type, &b.name))),
        }
        Ok(out)
    }

    fn add_concept(&self, name: &str, concept_type: &str) -> Result<()> {
        let mut s = self.lock();
        if s.concepts.iter().any(|c| c.name == name) {
            return Err(AppError::Invalid(format!("Concept '{name}' already exists")));
        }
        s.next_concept += 1;
        let id = s.next_concept;
        s.concepts.push(Concept {
            id: Some(id),
            name: name.to_string(),
            concept_type: concept_type.to_string(),
        });
        s.bump();
        Ok(())
    }

    fn set_budget(&self, concept: &str, limit: f64, period: &str) -> Result<()> {
        let mut s = self.lock();
        s.check_concept(concept)?;
        match s.budgets.iter_mut().find(|b| b.concept == concept && b.period == period) {
            Some(b) => b.monthly_limit = limit,
            None => {
                s.next_budget += 1;
                let id = s.next_budget;
                s.budgets.push(Budget {
                    id: Some(id),
                    concept: concept.to_string(),
                    monthly_limit: limit,
                    period: period.to_string(),
                });
            }
        }
        s.bump();
        Ok(())
    }

    fn list_budgets(&self, period: Option<&str>) -> Result<Vec<Budget>> {
        let s = self.lock();
        let mut out: Vec<Budget> = s
            .budgets
            .iter()
            .filter(|b| period.is_none_or(|p| b.period == p))
            .cloned()
            .collect();
        out.sort_by(|a, b| (&a.concept, &a.period).cmp(&(&b.concept, &b.period)));
        Ok(out)
    }

    fn delete_budget(&self, concept: &str, period: &str) -> Result<()> {
        let mut s = self.lock();
        s.budgets.retain(|b| !(b.concept == concept && b.period == period));
        s.bump();
        Ok(())
    }

    fn status(&self) -> Result<LedgerStatus> {
        Ok(LedgerStatus {
            revision: self.lock().revision,
            schema_version: crate::schema::EXPECTED_SCHEMA_VERSION,
        })
    }

    fn export_snapshot(&self) -> Result<LedgerSnapshot> {
        let s = self.lock();
        let mut accounts = s.accounts.clone();
        accounts.sort_by_key(|a| a.id);
        let mut concepts = s.concepts.clone();
        concepts.sort_by_key(|c| c.id);
        let mut budgets = s.budgets.clone();
        budgets.sort_by_key(|b| b.id);
        let mut entries: Vec<Entry> = s
            .entries
            .iter()
            .map(|r| Entry { from_account: None, to_account: None, ..s.to_entry(r) })
            .collect();
        entries.sort_by_key(|e| e.id);
        Ok(LedgerSnapshot {
            revision: s.revision,
            schema_version: crate::schema::EXPECTED_SCHEMA_VERSION,
            exported_at: chrono::Local::now().to_rfc3339(),
            concepts,
            accounts,
            entries,
            budgets,
            config: s.config.iter().map(|(k, v)| Config { key: k.clone(), value: v.clone() }).collect(),
        })
    }

    fn session_email(&self) -> Option<String> {
        Some("test@example.com".into())
    }
}
