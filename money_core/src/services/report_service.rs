use rusqlite::Connection;

use crate::error::Result;
use crate::models::{Bucket, Budget, TransactionSummary};

#[derive(Debug, Clone)]
pub struct MonthlyReport {
    pub month: i32,
    pub year: i32,
    pub total_income: f64,
    pub total_expense: f64,
    pub net_flow: f64,
    pub by_concept: Vec<TransactionSummary>,
    pub budgets: Vec<BudgetVsActual>,
    pub emergency_pct: f64,
}

#[derive(Debug, Clone)]
pub struct BudgetVsActual {
    pub concept: String,
    pub budgeted: f64,
    pub actual: f64,
    pub pct: f64,
}

#[derive(Debug, Clone)]
pub struct FullStatus {
    pub report: MonthlyReport,
    pub buckets: Vec<Bucket>,
    pub bucket_contributions: f64,
    pub bucket_withdrawals: f64,
    pub flujo: f64,
}

const EXCLUDE_FONDO: &str = "AND (tipo IS NULL OR tipo != 'Fondo')";

pub fn monthly_report(conn: &Connection, month: i32, year: i32) -> Result<MonthlyReport> {
    let total_income: f64 = conn.query_row(
        &format!(
            "SELECT COALESCE(SUM(amount), 0) FROM transactions
             WHERE amount > 0 AND month = ?1 AND year = ?2 {EXCLUDE_FONDO}"
        ),
        rusqlite::params![month, year],
        |row| row.get(0),
    )?;

    let total_expense: f64 = conn.query_row(
        &format!(
            "SELECT COALESCE(SUM(ABS(amount)), 0) FROM transactions
             WHERE amount < 0 AND month = ?1 AND year = ?2 {EXCLUDE_FONDO}"
        ),
        rusqlite::params![month, year],
        |row| row.get(0),
    )?;

    let net_flow = total_income - total_expense;

    let mut stmt = conn.prepare(
        &format!(
            "SELECT concept, SUM(ABS(amount)), COUNT(*)
             FROM transactions
             WHERE amount < 0 AND month = ?1 AND year = ?2 {EXCLUDE_FONDO}
             GROUP BY concept
             ORDER BY SUM(ABS(amount)) DESC"
        ),
    )?;
    let rows = stmt.query_map(rusqlite::params![month, year], |row| {
        Ok(TransactionSummary {
            concept: row.get(0)?,
            total: row.get(1)?,
            count: row.get(2)?,
        })
    })?;
    let mut by_concept = Vec::new();
    for row in rows {
        by_concept.push(row?);
    }

    let mut stmt2 = conn.prepare(
        "SELECT concept, monthly_limit FROM budgets WHERE month = ?1 AND year = ?2",
    )?;
    let budget_rows = stmt2.query_map(rusqlite::params![month, year], |row| {
        Ok(Budget {
            id: None,
            concept: row.get(0)?,
            monthly_limit: row.get(1)?,
            month,
            year,
        })
    })?;
    let mut budgets = Vec::new();
    for b in budget_rows {
        let b = b?;
        let actual: f64 = conn.query_row(
            &format!(
                "SELECT COALESCE(SUM(ABS(amount)), 0) FROM transactions
                 WHERE amount < 0 AND concept = ?1 AND month = ?2 AND year = ?3 {EXCLUDE_FONDO}"
            ),
            rusqlite::params![b.concept, month, year],
            |row| row.get(0),
        )?;
        let pct = if b.monthly_limit > 0.0 {
            (actual / b.monthly_limit * 100.0).min(999.0)
        } else {
            0.0
        };
        budgets.push(BudgetVsActual {
            concept: b.concept,
            budgeted: b.monthly_limit,
            actual,
            pct,
        });
    }

    let emergency_pct: f64 = conn
        .query_row(
            "SELECT COALESCE(value, '10') FROM config WHERE key = 'emergency_pct'",
            [],
            |row| {
                let v: String = row.get(0)?;
                Ok(v.parse::<f64>().unwrap_or(10.0))
            },
        )
        .unwrap_or(10.0);

    Ok(MonthlyReport {
        month,
        year,
        total_income,
        total_expense,
        net_flow,
        by_concept,
        budgets,
        emergency_pct,
    })
}

pub fn full_status(conn: &Connection, month: i32, year: i32) -> Result<FullStatus> {
    let report = monthly_report(conn, month, year)?;
    let buckets = crate::services::bucket_service::list_buckets(conn)?;

    let bucket_contributions: f64 = conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM bucket_movements WHERE amount > 0 AND month = ?1 AND year = ?2",
        rusqlite::params![month, year],
        |row| row.get(0),
    )?;

    let bucket_withdrawals: f64 = conn.query_row(
        "SELECT COALESCE(SUM(ABS(amount)), 0) FROM bucket_movements WHERE amount < 0 AND month = ?1 AND year = ?2",
        rusqlite::params![month, year],
        |row| row.get(0),
    )?;

    let flujo = report.total_income - report.total_expense - bucket_contributions + bucket_withdrawals;

    Ok(FullStatus {
        report,
        buckets,
        bucket_contributions,
        bucket_withdrawals,
        flujo,
    })
}
