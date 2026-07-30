use rusqlite::Connection;

use crate::error::Result;
use crate::models::{Bucket, BucketMovement};

pub fn create_bucket(conn: &Connection, b: &Bucket) -> Result<i64> {
    conn.execute(
        "INSERT INTO buckets (name, bucket_type, target_amount, savings_percentage, current_balance)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            b.name,
            b.bucket_type,
            b.target_amount,
            b.savings_percentage,
            b.current_balance,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_buckets(conn: &Connection) -> Result<Vec<Bucket>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, bucket_type, target_amount, savings_percentage, current_balance
         FROM buckets ORDER BY bucket_type, name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Bucket {
            id: Some(row.get(0)?),
            name: row.get(1)?,
            bucket_type: row.get(2)?,
            target_amount: row.get(3)?,
            savings_percentage: row.get(4)?,
            current_balance: row.get(5)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn get_bucket_by_name(conn: &Connection, name: &str) -> Result<Bucket> {
    let mut stmt = conn.prepare(
        "SELECT id, name, bucket_type, target_amount, savings_percentage, current_balance
         FROM buckets WHERE name = ?1",
    )?;
    let bucket = stmt.query_row(rusqlite::params![name], |row| {
        Ok(Bucket {
            id: Some(row.get(0)?),
            name: row.get(1)?,
            bucket_type: row.get(2)?,
            target_amount: row.get(3)?,
            savings_percentage: row.get(4)?,
            current_balance: row.get(5)?,
        })
    })?;
    Ok(bucket)
}

pub fn deposit_to_bucket(
    conn: &Connection,
    bucket_id: i64,
    amount: f64,
    date: &str,
    description: Option<&str>,
    month: i32,
    year: i32,
) -> Result<()> {
    conn.execute(
        "UPDATE buckets SET current_balance = current_balance + ?1 WHERE id = ?2",
        rusqlite::params![amount, bucket_id],
    )?;
    conn.execute(
        "INSERT INTO bucket_movements (bucket_id, date, amount, description, month, year)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![bucket_id, date, amount, description, month, year],
    )?;
    Ok(())
}

pub fn withdraw_from_bucket(
    conn: &Connection,
    bucket_id: i64,
    amount: f64,
    date: &str,
    description: Option<&str>,
    month: i32,
    year: i32,
) -> Result<()> {
    let balance: f64 = conn.query_row(
        "SELECT current_balance FROM buckets WHERE id = ?1",
        rusqlite::params![bucket_id],
        |row| row.get(0),
    )?;
    if balance < amount {
        let b = get_bucket_by_name(
            conn,
            &conn.query_row::<String, _, _>(
                "SELECT name FROM buckets WHERE id = ?1",
                rusqlite::params![bucket_id],
                |row| row.get(0),
            )?,
        )?;
        return Err(crate::error::AppError::Config(format!(
            "Insufficient balance in '{}': have ${:.2}, need ${:.2}",
            b.name, balance, amount
        )));
    }
    conn.execute(
        "UPDATE buckets SET current_balance = current_balance - ?1 WHERE id = ?2",
        rusqlite::params![amount, bucket_id],
    )?;
    conn.execute(
        "INSERT INTO bucket_movements (bucket_id, date, amount, description, month, year)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![bucket_id, date, -amount, description, month, year],
    )?;
    Ok(())
}

pub fn get_movements(
    conn: &Connection,
    bucket_id: i64,
) -> Result<Vec<BucketMovement>> {
    let mut stmt = conn.prepare(
        "SELECT id, bucket_id, date, amount, description, month, year
         FROM bucket_movements
         WHERE bucket_id = ?1
         ORDER BY date DESC",
    )?;
    let rows = stmt.query_map(rusqlite::params![bucket_id], |row| {
        Ok(BucketMovement {
            id: Some(row.get(0)?),
            bucket_id: row.get(1)?,
            date: row.get(2)?,
            amount: row.get(3)?,
            description: row.get(4)?,
            month: row.get(5)?,
            year: row.get(6)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}
