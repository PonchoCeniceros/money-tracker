use rusqlite::Connection;

use crate::error::Result;
use crate::models::Transaction;
use crate::services::{bucket_service, transaction_service};

pub fn init_flujo(conn: &Connection, amount: f64, date: &str, month: i32, year: i32) -> Result<i64> {
    let txn = Transaction {
        id: None,
        date: date.to_string(),
        amount,
        concept: "Saldo inicial".to_string(),
        subconcept: None,
        tipo: None,
        description: Some("Carga de saldo inicial".to_string()),
        month,
        year,
    };
    transaction_service::add_income(conn, &txn)
}

pub fn init_bucket_balance(
    conn: &Connection,
    bucket_id: i64,
    amount: f64,
    date: &str,
    month: i32,
    year: i32,
) -> Result<()> {
    bucket_service::deposit_to_bucket(
        conn,
        bucket_id,
        amount,
        date,
        Some("Saldo inicial"),
        month,
        year,
    )
}
