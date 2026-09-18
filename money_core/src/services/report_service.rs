use serde::Serialize;

use crate::error::Result;
use crate::models::{AccountBalance, ConceptSummary, EntryFilter};
use crate::period::Period;
use crate::storage::{ledger, LedgerBackend};

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

/// A month's activity, computed purely over `raw_accounts` + `entries` +
/// `budgets` via [`ledger::monthly_report`] — backend-agnostic by
/// construction.
pub fn monthly_report(be: &dyn LedgerBackend, period: &Period) -> Result<MonthlyReport> {
    let accounts = be.raw_accounts(false)?;
    let entries = be.entries(&EntryFilter {
        period: Some(period.clone()),
        ..Default::default()
    })?;
    let budgets = be.list_budgets(Some(period.as_str()))?;
    Ok(ledger::monthly_report(period, &accounts, &entries, &budgets))
}

/// Balances as of now, or, when `as_of` is given, recomputed per account up
/// to that date (inclusive). The as-of path is an in-memory derivation over
/// each account's entries rather than a per-account SQL query — identical
/// math on either backend.
pub fn net_worth(be: &dyn LedgerBackend, as_of: Option<&str>) -> Result<NetWorth> {
    let accounts = be.accounts_with_balances(false, as_of)?;
    Ok(ledger::net_worth(&accounts))
}

pub fn full_status(be: &dyn LedgerBackend, period: &Period) -> Result<FullStatus> {
    Ok(FullStatus {
        report: monthly_report(be, period)?,
        net_worth: net_worth(be, None)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NewAccount;
    use crate::services::{account_service, entry_service};
    use crate::storage::sqlite::SqliteBackend;

    fn setup() -> SqliteBackend {
        SqliteBackend::open_memory().unwrap()
    }

    #[test]
    fn credit_cycle_across_two_months_does_not_double_count() {
        let be = setup();
        let debito = account_service::create_account(&be, &NewAccount::spending("debito")).unwrap();
        let tdc =
            account_service::create_account(&be, &NewAccount::credit("tdc", Some(30000.0)))
                .unwrap();
        entry_service::add_income(&be, "2026-08-01", 10000.0, debito, "Nomina", None).unwrap();
        entry_service::add_expense(&be, "2026-08-12", 1800.0, tdc, "Discrecional", None, None)
            .unwrap();
        entry_service::add_transfer(&be, "2026-09-05", 1800.0, debito, tdc, None).unwrap();

        let august = monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
        assert_eq!(august.total_expense, 1800.0);
        assert_eq!(august.cash_out, 0.0);
        assert_eq!(august.on_credit, 1800.0);

        let september = monthly_report(&be, &Period::parse("2026-09").unwrap()).unwrap();
        assert_eq!(september.total_expense, 0.0);
        assert_eq!(september.cash_out, 1800.0);
    }

    #[test]
    fn expense_from_savings_is_flagged_and_not_cash_out() {
        let be = setup();
        let fondo = account_service::create_account(&be, &NewAccount::emergency("fondo")).unwrap();
        entry_service::add_opening(&be, "2026-08-01", 35000.0, fondo).unwrap();
        entry_service::add_expense(&be, "2026-08-19", 4200.0, fondo, "Servicios", None, None)
            .unwrap();

        let report = monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
        assert_eq!(report.total_expense, 4200.0);
        assert_eq!(report.from_savings, 4200.0);
        assert_eq!(report.cash_out, 0.0);
    }

    #[test]
    fn transfer_between_spending_accounts_does_not_affect_expense() {
        let be = setup();
        let debito = account_service::create_account(&be, &NewAccount::spending("debito")).unwrap();
        let efectivo =
            account_service::create_account(&be, &NewAccount::spending("efectivo")).unwrap();
        entry_service::add_income(&be, "2026-08-01", 5000.0, debito, "Nomina", None).unwrap();
        entry_service::add_transfer(&be, "2026-08-02", 1000.0, debito, efectivo, None).unwrap();

        let report = monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
        assert_eq!(report.total_expense, 0.0);
        assert_eq!(report.cash_out, 0.0);
    }

    #[test]
    fn net_worth_partitions_by_kind() {
        let be = setup();
        let debito = account_service::create_account(&be, &NewAccount::spending("debito")).unwrap();
        let fondo = account_service::create_account(&be, &NewAccount::emergency("fondo")).unwrap();
        let tdc =
            account_service::create_account(&be, &NewAccount::credit("tdc", Some(30000.0)))
                .unwrap();
        entry_service::add_income(&be, "2026-08-01", 10000.0, debito, "Nomina", None).unwrap();
        entry_service::add_opening(&be, "2026-08-01", 5000.0, fondo).unwrap();
        entry_service::add_expense(&be, "2026-08-02", 1200.0, tdc, "Discrecional", None, None)
            .unwrap();

        let nw = net_worth(&be, None).unwrap();
        assert_eq!(nw.cash_on_hand, 10000.0);
        assert_eq!(nw.savings, 5000.0);
        assert_eq!(nw.credit_debt, 1200.0);
        assert_eq!(nw.net, 10000.0 + 5000.0 - 1200.0);
    }

    #[test]
    fn budget_vs_actual_join() {
        let be = setup();
        let debito = account_service::create_account(&be, &NewAccount::spending("debito")).unwrap();
        entry_service::add_income(&be, "2026-08-01", 5000.0, debito, "Nomina", None).unwrap();
        entry_service::add_expense(&be, "2026-08-05", 2000.0, debito, "Alimentos", None, None)
            .unwrap();
        be.set_budget("Alimentos", 2500.0, "2026-08").unwrap();

        let report = monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
        assert_eq!(report.budgets.len(), 1);
        assert_eq!(report.budgets[0].actual, 2000.0);
        assert_eq!(report.budgets[0].pct, 80.0);
    }
}