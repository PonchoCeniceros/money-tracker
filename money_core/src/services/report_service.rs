use rusqlite::Connection;
use serde::Serialize;

use crate::error::Result;
use crate::models::{AccountBalance, AccountKind, ConceptSummary};
use crate::period::Period;
use crate::services::account_service;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct BudgetVsActual {
    pub concept: String,
    pub budgeted: f64,
    pub actual: f64,
    pub pct: f64,
}

/// A month's activity. `total_expense` (accrued) is what budgets compare
/// against; `cash_out` is what actually left spending accounts that month.
/// The two differ exactly when a card was charged (accrued but no cash
/// left), a card was paid (cash left, nothing was newly consumed), or an
/// expense was funded straight from savings (accrued but no cash left).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct MonthlyReport {
    pub period: Period,
    pub total_income: f64,
    pub total_expense: f64,
    pub cash_out: f64,
    pub net_flow: f64,
    pub by_concept: Vec<ConceptSummary>,
    pub budgets: Vec<BudgetVsActual>,
    /// Expense funded from an emergency/target account. Detail-only.
    pub from_savings: f64,
    /// Expense charged to a credit account. Detail-only.
    pub on_credit: f64,
    /// Transfers spending -> emergency/target this month (deposits).
    pub savings_contributions: f64,
    /// Transfers emergency/target -> spending this month (withdrawals to
    /// cash — NOT expenses; see the bucket-withdraw vs. direct-spend note).
    pub savings_withdrawals: f64,
    /// Transfers spending -> credit this month (paying down the card).
    pub card_payments: f64,
}

/// Point-in-time balances across all (non-archived) accounts.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct NetWorth {
    pub accounts: Vec<AccountBalance>,
    /// Sum of `spending` account balances — the real, carry-forward "flujo".
    pub cash_on_hand: f64,
    /// Sum of `emergency` + `target` account balances.
    pub savings: f64,
    /// Sum of credit account debt, reported as a positive number.
    pub credit_debt: f64,
    pub net: f64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct FullStatus {
    pub report: MonthlyReport,
    pub net_worth: NetWorth,
}

const EXPENSE_FROM_SPENDING: &str = "
    SELECT COALESCE(SUM(e.amount), 0) FROM entries e
    JOIN accounts fa ON fa.id = e.from_account_id
    WHERE e.kind = 'expense' AND fa.kind = 'spending' AND e.date >= ?1 AND e.date < ?2";

const CARD_PAYMENTS: &str = "
    SELECT COALESCE(SUM(e.amount), 0) FROM entries e
    JOIN accounts fa ON fa.id = e.from_account_id
    JOIN accounts ta ON ta.id = e.to_account_id
    WHERE e.kind = 'transfer' AND fa.kind = 'spending' AND ta.kind = 'credit'
      AND e.date >= ?1 AND e.date < ?2";

const EXPENSE_FROM_SAVINGS: &str = "
    SELECT COALESCE(SUM(e.amount), 0) FROM entries e
    JOIN accounts fa ON fa.id = e.from_account_id
    WHERE e.kind = 'expense' AND fa.kind IN ('emergency', 'target')
      AND e.date >= ?1 AND e.date < ?2";

const EXPENSE_ON_CREDIT: &str = "
    SELECT COALESCE(SUM(e.amount), 0) FROM entries e
    JOIN accounts fa ON fa.id = e.from_account_id
    WHERE e.kind = 'expense' AND fa.kind = 'credit'
      AND e.date >= ?1 AND e.date < ?2";

const SAVINGS_CONTRIBUTIONS: &str = "
    SELECT COALESCE(SUM(e.amount), 0) FROM entries e
    JOIN accounts fa ON fa.id = e.from_account_id
    JOIN accounts ta ON ta.id = e.to_account_id
    WHERE e.kind = 'transfer' AND fa.kind = 'spending' AND ta.kind IN ('emergency', 'target')
      AND e.date >= ?1 AND e.date < ?2";

const SAVINGS_WITHDRAWALS: &str = "
    SELECT COALESCE(SUM(e.amount), 0) FROM entries e
    JOIN accounts fa ON fa.id = e.from_account_id
    JOIN accounts ta ON ta.id = e.to_account_id
    WHERE e.kind = 'transfer' AND fa.kind IN ('emergency', 'target') AND ta.kind = 'spending'
      AND e.date >= ?1 AND e.date < ?2";

pub fn monthly_report(conn: &Connection, period: &Period) -> Result<MonthlyReport> {
    let start = period.start();
    let end = period.end_exclusive();

    let total_income: f64 = conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM entries
          WHERE kind = 'income' AND date >= ?1 AND date < ?2",
        rusqlite::params![start, end],
        |row| row.get(0),
    )?;

    let total_expense: f64 = conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM entries
          WHERE kind = 'expense' AND date >= ?1 AND date < ?2",
        rusqlite::params![start, end],
        |row| row.get(0),
    )?;

    let expense_cash: f64 = conn.query_row(EXPENSE_FROM_SPENDING, rusqlite::params![start, end], |row| row.get(0))?;
    let card_payments: f64 = conn.query_row(CARD_PAYMENTS, rusqlite::params![start, end], |row| row.get(0))?;
    let cash_out = expense_cash + card_payments;

    let from_savings: f64 = conn.query_row(EXPENSE_FROM_SAVINGS, rusqlite::params![start, end], |row| row.get(0))?;
    let on_credit: f64 = conn.query_row(EXPENSE_ON_CREDIT, rusqlite::params![start, end], |row| row.get(0))?;
    let savings_contributions: f64 =
        conn.query_row(SAVINGS_CONTRIBUTIONS, rusqlite::params![start, end], |row| row.get(0))?;
    let savings_withdrawals: f64 =
        conn.query_row(SAVINGS_WITHDRAWALS, rusqlite::params![start, end], |row| row.get(0))?;

    let mut stmt = conn.prepare(
        "SELECT concept, SUM(amount), COUNT(*)
           FROM entries
          WHERE kind = 'expense' AND date >= ?1 AND date < ?2
          GROUP BY concept
          ORDER BY SUM(amount) DESC",
    )?;
    let rows = stmt.query_map(rusqlite::params![start, end], |row| {
        Ok(ConceptSummary {
            concept: row.get(0)?,
            total: row.get(1)?,
            count: row.get(2)?,
        })
    })?;
    let mut by_concept = Vec::new();
    for row in rows {
        by_concept.push(row?);
    }

    // Single LEFT JOIN instead of one SELECT SUM per budget row.
    let mut stmt2 = conn.prepare(
        "SELECT b.concept, b.monthly_limit, COALESCE(a.spent, 0)
           FROM budgets b
           LEFT JOIN (
               SELECT concept, SUM(amount) AS spent FROM entries
                WHERE kind = 'expense' AND date >= ?1 AND date < ?2
                GROUP BY concept
           ) a ON a.concept = b.concept
          WHERE b.period = ?3
          ORDER BY b.concept",
    )?;
    let budget_rows = stmt2.query_map(rusqlite::params![start, end, period.as_str()], |row| {
        let budgeted: f64 = row.get(1)?;
        let actual: f64 = row.get(2)?;
        Ok(BudgetVsActual {
            concept: row.get(0)?,
            budgeted,
            actual,
            pct: if budgeted > 0.0 {
                (actual / budgeted * 100.0).min(999.0)
            } else {
                0.0
            },
        })
    })?;
    let mut budgets = Vec::new();
    for row in budget_rows {
        budgets.push(row?);
    }

    Ok(MonthlyReport {
        period: period.clone(),
        total_income,
        total_expense,
        cash_out,
        net_flow: total_income - total_expense,
        by_concept,
        budgets,
        from_savings,
        on_credit,
        savings_contributions,
        savings_withdrawals,
        card_payments,
    })
}

/// Balances as of now (derived from `account_balances`), or, when `as_of` is
/// given, recomputed per account up to that date. The latter is an N+1 over
/// accounts rather than a single query — acceptable at the scale of a
/// handful of personal accounts, unlike the old per-budget-row N+1 this
/// module used to have.
pub fn net_worth(conn: &Connection, as_of: Option<&str>) -> Result<NetWorth> {
    let accounts = match as_of {
        None => account_service::list_accounts(conn, false)?,
        Some(date) => {
            let mut accounts = account_service::list_accounts(conn, false)?;
            for account in &mut accounts {
                account.balance = account_service::balance_as_of(conn, account.id, date)?;
            }
            accounts
        }
    };

    let cash_on_hand: f64 = accounts
        .iter()
        .filter(|a| a.kind == AccountKind::Spending)
        .map(|a| a.balance)
        .sum();
    let savings: f64 = accounts
        .iter()
        .filter(|a| matches!(a.kind, AccountKind::Emergency | AccountKind::Target))
        .map(|a| a.balance)
        .sum();
    let credit_debt: f64 = accounts.iter().map(|a| a.debt()).sum();

    Ok(NetWorth {
        accounts,
        cash_on_hand,
        savings,
        credit_debt,
        net: cash_on_hand + savings - credit_debt,
    })
}

pub fn full_status(conn: &Connection, period: &Period) -> Result<FullStatus> {
    Ok(FullStatus {
        report: monthly_report(conn, period)?,
        net_worth: net_worth(conn, None)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::NewAccount;
    use crate::services::{account_service, entry_service};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        db::test_support::create_schema_for_tests(&conn);
        conn
    }

    #[test]
    fn credit_cycle_across_two_months_does_not_double_count() {
        let conn = setup();
        let debito = account_service::create_account(&conn, &NewAccount::spending("debito")).unwrap();
        let tdc = account_service::create_account(&conn, &NewAccount::credit("tdc", Some(30000.0))).unwrap();
        entry_service::add_income(&conn, "2026-08-01", 10000.0, debito, "Nomina", None).unwrap();
        entry_service::add_expense(&conn, "2026-08-12", 1800.0, tdc, "Discrecional", None, None).unwrap();
        entry_service::add_transfer(&conn, "2026-09-05", 1800.0, debito, tdc, None).unwrap();

        let august = monthly_report(&conn, &Period::parse("2026-08").unwrap()).unwrap();
        assert_eq!(august.total_expense, 1800.0);
        assert_eq!(august.cash_out, 0.0);
        assert_eq!(august.on_credit, 1800.0);

        let september = monthly_report(&conn, &Period::parse("2026-09").unwrap()).unwrap();
        assert_eq!(september.total_expense, 0.0);
        assert_eq!(september.cash_out, 1800.0);
    }

    #[test]
    fn expense_from_savings_is_flagged_and_not_cash_out() {
        let conn = setup();
        let fondo = account_service::create_account(&conn, &NewAccount::emergency("fondo")).unwrap();
        entry_service::add_opening(&conn, "2026-08-01", 35000.0, fondo).unwrap();
        entry_service::add_expense(&conn, "2026-08-19", 4200.0, fondo, "Servicios", None, None).unwrap();

        let report = monthly_report(&conn, &Period::parse("2026-08").unwrap()).unwrap();
        assert_eq!(report.total_expense, 4200.0);
        assert_eq!(report.from_savings, 4200.0);
        assert_eq!(report.cash_out, 0.0);
    }

    #[test]
    fn transfer_between_spending_accounts_does_not_affect_expense() {
        let conn = setup();
        let debito = account_service::create_account(&conn, &NewAccount::spending("debito")).unwrap();
        let efectivo = account_service::create_account(&conn, &NewAccount::spending("efectivo")).unwrap();
        entry_service::add_income(&conn, "2026-08-01", 5000.0, debito, "Nomina", None).unwrap();
        entry_service::add_transfer(&conn, "2026-08-02", 1000.0, debito, efectivo, None).unwrap();

        let report = monthly_report(&conn, &Period::parse("2026-08").unwrap()).unwrap();
        assert_eq!(report.total_expense, 0.0);
        assert_eq!(report.cash_out, 0.0);
    }

    #[test]
    fn net_worth_partitions_by_kind() {
        let conn = setup();
        let debito = account_service::create_account(&conn, &NewAccount::spending("debito")).unwrap();
        let fondo = account_service::create_account(&conn, &NewAccount::emergency("fondo")).unwrap();
        let tdc = account_service::create_account(&conn, &NewAccount::credit("tdc", Some(30000.0))).unwrap();
        entry_service::add_income(&conn, "2026-08-01", 10000.0, debito, "Nomina", None).unwrap();
        entry_service::add_opening(&conn, "2026-08-01", 5000.0, fondo).unwrap();
        entry_service::add_expense(&conn, "2026-08-02", 1200.0, tdc, "Discrecional", None, None).unwrap();

        let nw = net_worth(&conn, None).unwrap();
        assert_eq!(nw.cash_on_hand, 10000.0);
        assert_eq!(nw.savings, 5000.0);
        assert_eq!(nw.credit_debt, 1200.0);
        assert_eq!(nw.net, 10000.0 + 5000.0 - 1200.0);
    }

    #[test]
    fn budget_vs_actual_join() {
        let conn = setup();
        let debito = account_service::create_account(&conn, &NewAccount::spending("debito")).unwrap();
        entry_service::add_income(&conn, "2026-08-01", 5000.0, debito, "Nomina", None).unwrap();
        entry_service::add_expense(&conn, "2026-08-05", 2000.0, debito, "Alimentos", None, None).unwrap();
        conn.execute(
            "INSERT INTO budgets (concept, monthly_limit, period) VALUES ('Alimentos', 2500, '2026-08')",
            [],
        )
        .unwrap();

        let report = monthly_report(&conn, &Period::parse("2026-08").unwrap()).unwrap();
        assert_eq!(report.budgets.len(), 1);
        assert_eq!(report.budgets[0].actual, 2000.0);
        assert_eq!(report.budgets[0].pct, 80.0);
    }
}
