use rusqlite::Connection;
use std::path::PathBuf;

use crate::error::Result;

pub fn open_db() -> Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

fn db_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let mut path = PathBuf::from(home);
    path.push(".money-tracker");
    path.push("data.db");
    path
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS concepts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            concept_type TEXT NOT NULL CHECK(concept_type IN ('expense', 'income', 'both')),
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS transactions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL,
            amount REAL NOT NULL,
            concept TEXT NOT NULL,
            subconcept TEXT,
            tipo TEXT,
            description TEXT,
            month INTEGER NOT NULL,
            year INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS buckets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            bucket_type TEXT NOT NULL CHECK(bucket_type IN ('emergency', 'target')),
            target_amount REAL,
            savings_percentage REAL,
            current_balance REAL NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS bucket_movements (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            bucket_id INTEGER NOT NULL REFERENCES buckets(id),
            date TEXT NOT NULL,
            amount REAL NOT NULL,
            description TEXT,
            month INTEGER NOT NULL,
            year INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS budgets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            concept TEXT NOT NULL,
            monthly_limit REAL NOT NULL,
            month INTEGER NOT NULL,
            year INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )?;

    seed_concepts(conn)?;
    seed_config(conn)?;

    Ok(())
}

fn seed_concepts(conn: &Connection) -> Result<()> {
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM concepts", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(());
    }

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
        ("Saldo inicial", "income"),
    ];

    for (name, ctype) in &concepts {
        conn.execute(
            "INSERT INTO concepts (name, concept_type) VALUES (?1, ?2)",
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
    Ok(())
}
