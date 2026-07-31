use rusqlite::Connection;
use std::path::PathBuf;

use crate::error::{AppError, Result};

const SCHEMA_VERSION: i32 = 1;

pub fn open_db() -> Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    match version {
        v if v == SCHEMA_VERSION => {}
        0 if is_legacy_schema(&conn)? => return Err(AppError::LegacySchema { path }),
        0 => {
            init_schema(&conn)?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        v if v > SCHEMA_VERSION => {
            return Err(AppError::SchemaTooNew {
                found: v,
                expected: SCHEMA_VERSION,
            })
        }
        v => return Err(AppError::SchemaTooOld { found: v }),
    }

    Ok(conn)
}

/// Path to the SQLite database file. Honors `MONEY_TRACKER_DB` so tests and
/// verification sessions never touch the real `~/.money-tracker/data.db`.
pub fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("MONEY_TRACKER_DB") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let mut path = PathBuf::from(home);
    path.push(".money-tracker");
    path.push("data.db");
    path
}

/// v0 databases never set `user_version` and always have the old tables.
fn is_legacy_schema(conn: &Connection) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
          WHERE type='table' AND name IN ('transactions','buckets','bucket_movements')",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Creates the schema and seed data (concepts, `emergency_pct`) on a fresh
/// connection, without touching `user_version`. Exposed publicly so
/// integration tests, and future consumers like the GUI's own test suite,
/// can build a throwaway in-memory DB without going through `open_db`'s
/// filesystem path resolution.
pub fn init_schema(conn: &Connection) -> Result<()> {
    create_schema(conn)?;
    seed_concepts(conn)?;
    seed_config(conn)?;
    Ok(())
}

fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE concepts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            concept_type TEXT NOT NULL CHECK(concept_type IN ('expense', 'income', 'both')),
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE accounts (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            name          TEXT    NOT NULL UNIQUE,
            kind          TEXT    NOT NULL
                          CHECK (kind IN ('spending','emergency','target','credit')),
            target_amount REAL    CHECK (target_amount IS NULL OR target_amount > 0),
            credit_limit  REAL    CHECK (credit_limit  IS NULL OR credit_limit  > 0),
            liquid        INTEGER NOT NULL DEFAULT 1 CHECK (liquid   IN (0,1)),
            archived      INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
            created_at    TEXT    NOT NULL DEFAULT (datetime('now')),

            CHECK (kind =  'target' OR target_amount IS NULL),
            CHECK (kind <> 'target' OR target_amount IS NOT NULL),
            CHECK (kind =  'credit' OR credit_limit  IS NULL)
        );

        -- at most one active emergency account
        CREATE UNIQUE INDEX idx_accounts_one_emergency
            ON accounts(kind) WHERE kind = 'emergency' AND archived = 0;

        CREATE TABLE entries (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            date            TEXT    NOT NULL CHECK (date IS strftime('%Y-%m-%d', date)),
            kind            TEXT    NOT NULL
                            CHECK (kind IN ('income','expense','transfer','opening')),
            amount          REAL    NOT NULL CHECK (amount > 0),
            from_account_id INTEGER REFERENCES accounts(id),
            to_account_id   INTEGER REFERENCES accounts(id),
            concept         TEXT    REFERENCES concepts(name) ON UPDATE CASCADE,
            subconcept      TEXT,
            description     TEXT,
            created_at      TEXT    NOT NULL DEFAULT (datetime('now')),

            CHECK (
                (kind IN ('income','opening')
                     AND from_account_id IS NULL     AND to_account_id IS NOT NULL)
             OR (kind = 'expense'
                     AND from_account_id IS NOT NULL AND to_account_id IS NULL)
             OR (kind = 'transfer'
                     AND from_account_id IS NOT NULL AND to_account_id IS NOT NULL
                     AND from_account_id <> to_account_id)
            ),
            CHECK (kind IN ('transfer', 'opening') OR concept IS NOT NULL)
        );

        CREATE INDEX idx_entries_date         ON entries(date);
        CREATE INDEX idx_entries_kind_date    ON entries(kind, date);
        CREATE INDEX idx_entries_concept_date ON entries(concept, date) WHERE kind = 'expense';
        CREATE INDEX idx_entries_from ON entries(from_account_id) WHERE from_account_id IS NOT NULL;
        CREATE INDEX idx_entries_to   ON entries(to_account_id)   WHERE to_account_id   IS NOT NULL;

        CREATE VIEW account_balances AS
        SELECT a.id, a.name, a.kind, a.target_amount, a.credit_limit, a.liquid, a.archived,
               COALESCE(m.balance, 0.0) AS balance
        FROM accounts a
        LEFT JOIN (
            SELECT account_id, ROUND(SUM(delta), 2) AS balance
            FROM (
                SELECT to_account_id   AS account_id,  amount AS delta
                  FROM entries WHERE to_account_id   IS NOT NULL
                UNION ALL
                SELECT from_account_id AS account_id, -amount AS delta
                  FROM entries WHERE from_account_id IS NOT NULL
            ) GROUP BY account_id
        ) m ON m.account_id = a.id;

        CREATE VIEW entries_view AS
        SELECT e.*, fa.name AS from_account, ta.name AS to_account
        FROM entries e
        LEFT JOIN accounts fa ON fa.id = e.from_account_id
        LEFT JOIN accounts ta ON ta.id = e.to_account_id;

        CREATE TABLE budgets (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            concept       TEXT NOT NULL REFERENCES concepts(name) ON UPDATE CASCADE,
            monthly_limit REAL NOT NULL CHECK (monthly_limit > 0),
            period        TEXT NOT NULL CHECK (period IS strftime('%Y-%m', period || '-01')),
            UNIQUE (concept, period)
        );

        CREATE TABLE config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )?;
    Ok(())
}

fn seed_concepts(conn: &Connection) -> Result<()> {
    let concepts = [
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

    for (name, ctype) in &concepts {
        conn.execute(
            "INSERT OR IGNORE INTO concepts (name, concept_type) VALUES (?1, ?2)",
            rusqlite::params![name, ctype],
        )?;
    }
    Ok(())
}

fn seed_config(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO config (key, value) VALUES ('emergency_pct', '10')",
        [],
    )?;
    // default_account / income_account / cash_concept are left unset until
    // the user has created accounts (`account add` / `config set`).
    Ok(())
}

/// Test-only helper shared across `services/*`'s own `#[cfg(test)]` modules,
/// so every service's tests build the schema the same way `open_db` does.
#[cfg(test)]
pub(crate) mod test_support {
    use rusqlite::Connection;

    pub(crate) fn create_schema_for_tests(conn: &Connection) {
        super::create_schema(conn).unwrap();
        super::seed_concepts(conn).unwrap();
        super::seed_config(conn).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_memory() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        create_schema(&conn).unwrap();
        seed_concepts(&conn).unwrap();
        seed_config(&conn).unwrap();
        conn
    }

    #[test]
    fn schema_creates_cleanly() {
        let _conn = open_memory();
    }

    #[test]
    fn only_one_active_emergency_account() {
        let conn = open_memory();
        conn.execute(
            "INSERT INTO accounts (name, kind) VALUES ('fondo', 'emergency')",
            [],
        )
        .unwrap();
        let err = conn
            .execute(
                "INSERT INTO accounts (name, kind) VALUES ('fondo2', 'emergency')",
                [],
            )
            .unwrap_err();
        assert!(format!("{err}").contains("UNIQUE") || format!("{err}").contains("constraint"));
    }

    #[test]
    fn target_account_requires_target_amount() {
        let conn = open_memory();
        let err = conn
            .execute(
                "INSERT INTO accounts (name, kind) VALUES ('vacaciones', 'target')",
                [],
            )
            .unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("check"));
    }

    #[test]
    fn malformed_date_rejected() {
        let conn = open_memory();
        conn.execute(
            "INSERT INTO accounts (name, kind) VALUES ('efectivo', 'spending')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO concepts (name, concept_type) VALUES ('Alimentos2', 'expense')",
            [],
        )
        .ok();
        let err = conn
            .execute(
                "INSERT INTO entries (date, kind, amount, from_account_id, concept)
                 VALUES ('2026-7-4', 'expense', 100, 1, 'Alimentos')",
                [],
            )
            .unwrap_err();
        assert!(format!("{err}").to_lowercase().contains("check"));
    }
}
