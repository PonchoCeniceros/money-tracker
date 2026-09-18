use crate::error::Result;
use crate::models::{Entry, NewEntry};
use crate::storage::{round2, LedgerBackend};

// Re-exported because CLI/GUI import `EntryFilter`/`EntryUpdate` from here —
// the canonical definitions live in `models` so the mirror/sync layer can
// use them without a service dependency.
pub use crate::models::{EntryFilter, EntryUpdate};

/// Sole writer of `entries`. Every other constructor in this module funnels
/// through `LedgerBackend::push_entries`, so the invariants encoded in
/// `NewEntry`'s constructors (and the backend's atomic overdraft/credit
/// checks) are the only path onto the ledger — on SQLite OR Supabase.
pub fn add_income(
    be: &dyn LedgerBackend,
    date: &str,
    amount: f64,
    to: i64,
    concept: &str,
    description: Option<&str>,
) -> Result<i64> {
    let entry = NewEntry::income(date, amount, to, concept)?.with_description(description);
    Ok(be.push_entries(&[entry])?[0].id)
}

pub fn add_expense(
    be: &dyn LedgerBackend,
    date: &str,
    amount: f64,
    from: i64,
    concept: &str,
    subconcept: Option<&str>,
    description: Option<&str>,
) -> Result<i64> {
    let entry = NewEntry::expense(date, amount, from, concept)?
        .with_subconcept(subconcept)
        .with_description(description);
    Ok(be.push_entries(&[entry])?[0].id)
}

pub fn add_transfer(
    be: &dyn LedgerBackend,
    date: &str,
    amount: f64,
    from: i64,
    to: i64,
    description: Option<&str>,
) -> Result<i64> {
    let entry = NewEntry::transfer(date, amount, from, to)?.with_description(description);
    Ok(be.push_entries(&[entry])?[0].id)
}

pub fn add_opening(be: &dyn LedgerBackend, date: &str, amount: f64, to: i64) -> Result<i64> {
    let entry = NewEntry::opening(date, amount, to)?;
    Ok(be.push_entries(&[entry])?[0].id)
}

pub struct IncomeResult {
    pub entry_id: i64,
    /// (account name, amount) transferred to the emergency fund, if any.
    pub emergency: Option<(String, f64)>,
}

/// Registers an income and, if the destination account is liquid and an
/// emergency account exists, auto-splits `emergency_pct`% into it.
///
/// Both the income and its split transfer go through a single atomic
/// `push_entries` batch, so the split cannot be half-applied on any backend
/// (SQLite runs it in `BEGIN IMMEDIATE`; Supabase's `apply_entries` in one
/// RPC call).
pub fn add_income_with_emergency_split(
    be: &dyn LedgerBackend,
    date: &str,
    amount: f64,
    to: i64,
    concept: &str,
    description: Option<&str>,
    split: bool,
) -> Result<IncomeResult> {
    let mut batch: Vec<NewEntry> = vec![NewEntry::income(date, amount, to, concept)?
        .with_description(description)];

    let mut emergency = None;
    if split {
        let to_account = be.get_account(to)?;
        if to_account.liquid {
            if let Some(fund) = be.emergency_account()? {
                if fund.id != to {
                    let pct: f64 = be
                        .get_config("emergency_pct")?
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(10.0);
                    let split_amount = round2(amount * pct / 100.0);
                    if split_amount > 0.0 {
                        batch.push(NewEntry::transfer(date, split_amount, to, fund.id)?);
                        emergency = Some((fund.name, split_amount));
                    }
                }
            }
        }
    }

    let inserted = be.push_entries(&batch)?;
    Ok(IncomeResult {
        entry_id: inserted[0].id,
        emergency,
    })
}

pub fn list(be: &dyn LedgerBackend, f: &EntryFilter) -> Result<Vec<Entry>> {
    be.entries(f)
}

pub fn get(be: &dyn LedgerBackend, id: i64) -> Result<Entry> {
    be.get_entry(id)
}

pub fn delete(be: &dyn LedgerBackend, id: i64) -> Result<()> {
    be.delete_entry(id)
}

/// Corrects an existing entry in place, preserving its id. Only the account
/// side that already applies to the entry's `kind` may be changed (e.g. an
/// expense has no `to_account_id` to set) — the kind itself never changes,
/// since that would make it a different kind of movement entirely, not a
/// correction of this one.
///
/// Skips re-running the overdraft/credit-limit guard that `add_expense`/
/// `add_transfer` apply on insert: this is a correction to historical data,
/// not a new movement, and the original entry already passed that check
/// once. The SQL CHECK constraints (kind/nullability, amount > 0, no
/// self-transfer, valid date) remain enforced as a backstop.
pub fn update(be: &dyn LedgerBackend, id: i64, upd: &EntryUpdate) -> Result<Entry> {
    be.update_entry(id, upd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EntryKind;
    use crate::storage::sqlite::SqliteBackend;

    fn setup() -> SqliteBackend {
        SqliteBackend::open_memory().unwrap()
    }

    fn make_account(be: &dyn LedgerBackend, name: &str, kind: &str) -> i64 {
        match kind {
            "spending" => be.insert_account(&crate::models::NewAccount::spending(name)).unwrap(),
            "emergency" => be.insert_account(&crate::models::NewAccount::emergency(name)).unwrap(),
            "credit" => be
                .insert_account(&crate::models::NewAccount::credit(name, None))
                .unwrap(),
            other => panic!("unknown kind {other}"),
        }
    }

    #[test]
    fn expense_and_income_move_balance() {
        let be = setup();
        let debito = make_account(&be, "debito", "spending");
        add_income(&be, "2026-08-01", 1000.0, debito, "Nomina", None).unwrap();
        add_expense(&be, "2026-08-02", 300.0, debito, "Alimentos", None, None).unwrap();
        let balances = be.list_accounts(false).unwrap();
        assert_eq!(balances[0].balance, 700.0);
    }

    #[test]
    fn transfer_does_not_change_total_across_two_accounts() {
        let be = setup();
        let debito = make_account(&be, "debito", "spending");
        let fondo = make_account(&be, "fondo", "emergency");
        add_income(&be, "2026-08-01", 1000.0, debito, "Nomina", None).unwrap();
        add_transfer(&be, "2026-08-02", 200.0, debito, fondo, None).unwrap();
        let balances = be.list_accounts(false).unwrap();
        let deb = balances.iter().find(|a| a.id == debito).unwrap().balance;
        let fon = balances.iter().find(|a| a.id == fondo).unwrap().balance;
        assert_eq!(deb, 800.0);
        assert_eq!(fon, 200.0);
    }

    #[test]
    fn emergency_withdrawal_over_balance_is_rejected() {
        let be = setup();
        let debito = make_account(&be, "debito", "spending");
        let fondo = make_account(&be, "fondo", "emergency");
        add_opening(&be, "2026-08-01", 100.0, fondo).unwrap();
        let err = add_transfer(&be, "2026-08-02", 500.0, fondo, debito, None);
        assert!(err.is_err());
    }

    #[test]
    fn self_transfer_is_rejected() {
        let be = setup();
        let debito = make_account(&be, "debito", "spending");
        add_income(&be, "2026-08-01", 100.0, debito, "Nomina", None).unwrap();
        let err = add_transfer(&be, "2026-08-02", 50.0, debito, debito, None);
        assert!(err.is_err());
    }

    #[test]
    fn credit_charge_over_limit_is_rejected() {
        let be = setup();
        let tdc = be.insert_account(&crate::models::NewAccount::credit("tdc", Some(1000.0))).unwrap();
        let err = add_expense(&be, "2026-08-02", 1500.0, tdc, "Discrecional", None, None);
        assert!(err.is_err());
        // under the limit still works
        add_expense(&be, "2026-08-03", 500.0, tdc, "Discrecional", None, None).unwrap();
    }

    #[test]
    fn credit_card_charge_and_payment_net_to_zero() {
        let be = setup();
        let debito = make_account(&be, "debito", "spending");
        let tdc = make_account(&be, "tdc", "credit");
        add_income(&be, "2026-08-01", 5000.0, debito, "Nomina", None).unwrap();
        add_expense(&be, "2026-08-12", 1800.0, tdc, "Discrecional", None, None).unwrap();
        add_transfer(&be, "2026-09-05", 1800.0, debito, tdc, None).unwrap();

        let balances = be.list_accounts(false).unwrap();
        let tdc_balance = balances.iter().find(|a| a.id == tdc).unwrap().balance;
        assert_eq!(tdc_balance, 0.0);

        // no double counting: total expense across both months is 1800, not 3600
        let total_expense: f64 = be
            .entries(&EntryFilter::default())
            .unwrap()
            .iter()
            .filter(|e| e.kind == EntryKind::Expense)
            .map(|e| e.amount)
            .sum();
        assert_eq!(total_expense, 1800.0);
    }

    #[test]
    fn opening_is_excluded_from_income_totals() {
        let be = setup();
        let debito = make_account(&be, "debito", "spending");
        add_opening(&be, "2026-08-01", 5000.0, debito).unwrap();
        let all = be.entries(&EntryFilter::default()).unwrap();
        let income_total: f64 = all
            .iter()
            .filter(|e| e.kind == EntryKind::Income)
            .map(|e| e.amount)
            .sum();
        assert_eq!(income_total, 0.0);
        let balance = be.get_account(debito).unwrap().balance;
        assert_eq!(balance, 5000.0);
    }

    #[test]
    fn emergency_split_skipped_for_restricted_account() {
        let be = setup();
        let vales = be
            .insert_account(&crate::models::NewAccount::spending("vales").restricted())
            .unwrap();
        let fondo = make_account(&be, "fondo", "emergency");

        let result =
            add_income_with_emergency_split(&be, "2026-08-01", 2400.0, vales, "Vales de Despensa", None, true)
                .unwrap();
        assert!(result.emergency.is_none());
        assert_eq!(be.get_account(fondo).unwrap().balance, 0.0);
    }

    #[test]
    fn emergency_split_applies_for_liquid_account() {
        let be = setup();
        let debito = make_account(&be, "debito", "spending");
        let fondo = make_account(&be, "fondo", "emergency");

        let result = add_income_with_emergency_split(&be, "2026-08-01", 24000.0, debito, "Nomina", None, true)
            .unwrap();
        assert_eq!(result.emergency, Some(("fondo".to_string(), 2400.0)));
        assert_eq!(be.get_account(fondo).unwrap().balance, 2400.0);
    }

    #[test]
    fn update_changes_amount_and_account_for_an_expense() {
        let be = setup();
        let debito = make_account(&be, "debito", "spending");
        let vales = make_account(&be, "vales", "spending");
        let id = add_expense(&be, "2026-08-01", 210.0, debito, "Alimentos", None, None).unwrap();

        let updated = update(
            &be,
            id,
            &EntryUpdate {
                amount: Some(177.0),
                from_account_id: Some(vales),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated.amount, 177.0);
        assert_eq!(updated.from_account_id, Some(vales));

        let debito_balance = be.get_account(debito).unwrap().balance;
        let vales_balance = be.get_account(vales).unwrap().balance;
        assert_eq!(debito_balance, 0.0);
        assert_eq!(vales_balance, -177.0);
    }

    #[test]
    fn update_rejects_setting_the_wrong_side_for_the_kind() {
        let be = setup();
        let debito = make_account(&be, "debito", "spending");
        let id = add_expense(&be, "2026-08-01", 210.0, debito, "Alimentos", None, None).unwrap();

        let err = update(
            &be,
            id,
            &EntryUpdate {
                to_account_id: Some(debito),
                ..Default::default()
            },
        );
        assert!(err.is_err());
    }

    #[test]
    fn update_rejects_turning_a_transfer_into_a_self_transfer() {
        let be = setup();
        let debito = make_account(&be, "debito", "spending");
        let fondo = make_account(&be, "fondo", "emergency");
        add_income(&be, "2026-08-01", 1000.0, debito, "Nomina", None).unwrap();
        let id = add_transfer(&be, "2026-08-02", 200.0, debito, fondo, None).unwrap();

        let err = update(
            &be,
            id,
            &EntryUpdate {
                to_account_id: Some(debito),
                ..Default::default()
            },
        );
        assert!(err.is_err());
    }

    #[test]
    fn update_on_missing_entry_is_not_found() {
        let be = setup();
        let err = update(&be, 999, &EntryUpdate::default());
        assert!(matches!(err, Err(crate::error::AppError::NotFound(_))));
    }
}