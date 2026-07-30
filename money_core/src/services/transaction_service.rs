use rusqlite::Connection;

use crate::error::{AppError, Result};
use crate::models::Transaction;

pub fn add_transaction(conn: &Connection, t: &Transaction) -> Result<i64> {
    conn.execute(
        "INSERT INTO transactions (date, amount, concept, subconcept, tipo, description, month, year)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            t.date,
            t.amount,
            t.concept,
            t.subconcept,
            t.tipo,
            t.description,
            t.month,
            t.year,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn add_income(conn: &Connection, t: &Transaction) -> Result<i64> {
    if t.amount <= 0.0 {
        return Err(AppError::Config("Income amount must be positive".into()));
    }
    add_transaction(conn, t)
}

pub fn add_expense(conn: &Connection, t: &Transaction) -> Result<i64> {
    if t.amount >= 0.0 {
        return Err(AppError::Config("Expense amount must be negative".into()));
    }
    add_transaction(conn, t)
}

pub fn list_transactions(
    conn: &Connection,
    month: i32,
    year: i32,
) -> Result<Vec<Transaction>> {
    let mut stmt = conn.prepare(
        "SELECT id, date, amount, concept, subconcept, tipo, description, month, year
         FROM transactions
         WHERE month = ?1 AND year = ?2
         ORDER BY date DESC",
    )?;
    let rows = stmt.query_map(rusqlite::params![month, year], |row| {
        Ok(Transaction {
            id: Some(row.get(0)?),
            date: row.get(1)?,
            amount: row.get(2)?,
            concept: row.get(3)?,
            subconcept: row.get(4)?,
            tipo: row.get(5)?,
            description: row.get(6)?,
            month: row.get(7)?,
            year: row.get(8)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn list_by_concept(
    conn: &Connection,
    concept: &str,
    month: i32,
    year: i32,
) -> Result<Vec<Transaction>> {
    let mut stmt = conn.prepare(
        "SELECT id, date, amount, concept, subconcept, tipo, description, month, year
         FROM transactions
         WHERE concept = ?1 AND month = ?2 AND year = ?3
         ORDER BY date DESC",
    )?;
    let rows = stmt.query_map(rusqlite::params![concept, month, year], |row| {
        Ok(Transaction {
            id: Some(row.get(0)?),
            date: row.get(1)?,
            amount: row.get(2)?,
            concept: row.get(3)?,
            subconcept: row.get(4)?,
            tipo: row.get(5)?,
            description: row.get(6)?,
            month: row.get(7)?,
            year: row.get(8)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}
