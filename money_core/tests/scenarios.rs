//! Black-box scenarios exercised through `money_core`'s public API only,
//! mirroring the verification plan. These complement each service's own
//! `#[cfg(test)]` unit tests (which can see private details) by proving the
//! crate's *external* contract holds even if internal structure changes.
//!
//! Everything runs against an in-memory `SqliteBackend`; the same service
//! layer talks to Supabase at runtime, so these scenarios pin the math that
//! must not drift between stores.

use money_core::models::NewAccount;
use money_core::period::Period;
use money_core::services::{account_service, entry_service, report_service, setup_service};
use money_core::services::setup_service::SeedOptions;
use money_core::storage::sqlite::SqliteBackend;
use money_core::LedgerBackend;

fn fresh_db() -> SqliteBackend {
    SqliteBackend::open_memory().unwrap()
}

/// V0/seed: opening balances must not show up as income in the month
/// they're loaded — the exact regression the old `init_service` had.
#[test]
fn seed_then_report_shows_zero_income() {
    let be = fresh_db();
    account_service::create_account(&be, &NewAccount::spending("efectivo")).unwrap();
    account_service::create_account(&be, &NewAccount::emergency("Fondo de emergencia")).unwrap();

    assert!(!setup_service::is_seeded(&be).unwrap());
    setup_service::seed(
        &be,
        &SeedOptions {
            accounts: vec![
                ("efectivo".into(), 1000.0),
                ("Fondo de emergencia".into(), 35000.0),
            ],
            date: "2026-08-01".into(),
        },
    )
    .unwrap();
    assert!(setup_service::is_seeded(&be).unwrap());

    let report = report_service::monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
    assert_eq!(report.total_income, 0.0);

    let nw = report_service::net_worth(&be, None).unwrap();
    assert_eq!(nw.cash_on_hand, 1000.0);
    assert_eq!(nw.savings, 35000.0);
}

/// Op (a): register an expense from the default-ish spending account.
#[test]
fn op_a_register_expense() {
    let be = fresh_db();
    let efectivo = account_service::create_account(&be, &NewAccount::spending("efectivo")).unwrap();
    entry_service::add_income(&be, "2026-08-01", 1000.0, efectivo, "Nomina", None).unwrap();

    entry_service::add_expense(&be, "2026-08-03", 350.0, efectivo, "Alimentos", None, None).unwrap();

    let account = account_service::get_account(&be, efectivo).unwrap();
    assert_eq!(account.balance, 650.0);
    let report = report_service::monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
    assert_eq!(report.total_expense, 350.0);
}

/// Op (b): register income, with the emergency auto-split — and the
/// restricted-account case where it must NOT fire (vales de despensa).
#[test]
fn op_b_register_income_with_emergency_split_and_restricted_account() {
    let be = fresh_db();
    let debito = account_service::create_account(&be, &NewAccount::spending("debito")).unwrap();
    let vales = account_service::create_account(
        &be,
        &NewAccount::spending("vales").restricted(),
    )
    .unwrap();
    let fondo =
        account_service::create_account(&be, &NewAccount::emergency("Fondo de emergencia")).unwrap();

    let result = entry_service::add_income_with_emergency_split(
        &be, "2026-08-05", 24000.0, debito, "Nomina", None, true,
    )
    .unwrap();
    assert_eq!(result.emergency.as_ref().unwrap().1, 2400.0);

    let result2 = entry_service::add_income_with_emergency_split(
        &be,
        "2026-08-05",
        2400.0,
        vales,
        "Vales de Despensa",
        None,
        true,
    )
    .unwrap();
    assert!(result2.emergency.is_none());

    let fondo_balance = account_service::get_account(&be, fondo).unwrap().balance;
    assert_eq!(fondo_balance, 2400.0); // only from the debito income, not vales
}

/// Op (c): deposit into a bucket must not appear as an expense.
#[test]
fn op_c_bucket_deposit_is_not_an_expense() {
    let be = fresh_db();
    let debito = account_service::create_account(&be, &NewAccount::spending("debito")).unwrap();
    let vacaciones =
        account_service::create_account(&be, &NewAccount::target("Vacaciones", Some(50000.0)).unwrap()).unwrap();
    entry_service::add_income(&be, "2026-08-01", 5000.0, debito, "Nomina", None).unwrap();

    entry_service::add_transfer(&be, "2026-08-02", 2000.0, debito, vacaciones, None).unwrap();

    let report = report_service::monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
    assert_eq!(report.total_expense, 0.0);
    let vacaciones_balance = account_service::get_account(&be, vacaciones).unwrap();
    assert_eq!(vacaciones_balance.balance, 2000.0);
    assert_eq!(vacaciones_balance.progress_pct(), Some(4.0));
}

/// Op (d): spending directly from a bucket (the Excel's constant
/// "tipo = Fondo emergencia" pattern) is one entry, flagged as
/// savings-funded, and does not move cash.
#[test]
fn op_d_expense_direct_from_emergency_bucket() {
    let be = fresh_db();
    let fondo =
        account_service::create_account(&be, &NewAccount::emergency("Fondo de emergencia")).unwrap();
    setup_service::seed(
        &be,
        &SeedOptions {
            accounts: vec![("Fondo de emergencia".into(), 35000.0)],
            date: "2026-08-01".into(),
        },
    )
    .unwrap();

    entry_service::add_expense(&be, "2026-08-19", 4200.0, fondo, "Servicios", None, None).unwrap();

    let report = report_service::monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
    assert_eq!(report.total_expense, 4200.0);
    assert_eq!(report.from_savings, 4200.0);
    assert_eq!(report.cash_out, 0.0);

    let fondo_balance = account_service::get_account(&be, fondo).unwrap().balance;
    assert_eq!(fondo_balance, 30800.0);
}

/// Op (e): budgets are informative only — exceeding one never blocks.
#[test]
fn op_e_budget_is_informative_only() {
    let be = fresh_db();
    let efectivo = account_service::create_account(&be, &NewAccount::spending("efectivo")).unwrap();
    entry_service::add_income(&be, "2026-08-01", 5000.0, efectivo, "Nomina", None).unwrap();
    be.set_budget("Alimentos", 2500.0, "2026-08").unwrap();

    // exceeds budget, must still succeed
    entry_service::add_expense(&be, "2026-08-05", 3000.0, efectivo, "Alimentos", None, None).unwrap();

    let report = report_service::monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
    let budget = &report.budgets[0];
    assert_eq!(budget.actual, 3000.0);
    assert!(budget.pct > 100.0);
}

/// The cash envelope: an ATM-style transfer to a cash account, then
/// reconciling what's physically left writes the untracked spend as one
/// expense entry against the configured concept.
#[test]
fn cash_envelope_transfer_then_reconcile() {
    let be = fresh_db();
    let debito = account_service::create_account(&be, &NewAccount::spending("debito")).unwrap();
    let efectivo = account_service::create_account(&be, &NewAccount::spending("efectivo")).unwrap();
    entry_service::add_income(&be, "2026-08-01", 18000.0, debito, "Nomina", None).unwrap();

    entry_service::add_transfer(&be, "2026-08-01", 1000.0, debito, efectivo, None).unwrap();
    let report = report_service::monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
    assert_eq!(report.total_expense, 0.0); // the envelope withdrawal is not an expense

    let result =
        account_service::reconcile_account(&be, efectivo, 150.0, "Discrecional", "2026-08-31").unwrap();
    assert_eq!(result.diff, 850.0);

    let efectivo_balance = account_service::get_account(&be, efectivo).unwrap().balance;
    assert_eq!(efectivo_balance, 150.0);
    let report = report_service::monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
    assert_eq!(report.total_expense, 850.0);
}

/// The critical two-month credit cycle: charge in month 1, pay in month 2,
/// no double counting across the pair.
#[test]
fn credit_card_cycle_across_two_months() {
    let be = fresh_db();
    let debito = account_service::create_account(&be, &NewAccount::spending("debito")).unwrap();
    let tdc = account_service::create_account(&be, &NewAccount::credit("tdc", Some(30000.0))).unwrap();
    entry_service::add_income(&be, "2026-08-01", 10000.0, debito, "Nomina", None).unwrap();

    entry_service::add_expense(&be, "2026-08-12", 1800.0, tdc, "Discrecional", None, None).unwrap();
    let august = report_service::monthly_report(&be, &Period::parse("2026-08").unwrap()).unwrap();
    assert_eq!(august.total_expense, 1800.0);
    assert_eq!(august.cash_out, 0.0);
    let tdc_balance = account_service::get_account(&be, tdc).unwrap();
    assert_eq!(tdc_balance.debt(), 1800.0);
    assert_eq!(tdc_balance.available_credit(), Some(28200.0));

    entry_service::add_transfer(&be, "2026-09-05", 1800.0, debito, tdc, None).unwrap();
    let september = report_service::monthly_report(&be, &Period::parse("2026-09").unwrap()).unwrap();
    assert_eq!(september.total_expense, 0.0);
    assert_eq!(september.cash_out, 1800.0);

    let discrecional_total: f64 = be
        .entries(&money_core::models::EntryFilter {
            concept: Some("Discrecional".into()),
            ..Default::default()
        })
        .unwrap()
        .iter()
        .filter(|e| e.kind == money_core::models::EntryKind::Expense)
        .map(|e| e.amount)
        .sum();
    assert_eq!(discrecional_total, 1800.0); // not 3600 — no double count
}