use crate::error::{AppError, Result};
use crate::models::{AccountBalance, NewAccount, NewEntry};
use crate::storage::LedgerBackend;

pub fn create_account(be: &dyn LedgerBackend, new: &NewAccount) -> Result<i64> {
    be.insert_account(new)
}

pub fn list_accounts(be: &dyn LedgerBackend, include_archived: bool) -> Result<Vec<AccountBalance>> {
    be.list_accounts(include_archived)
}

pub fn get_account(be: &dyn LedgerBackend, id: i64) -> Result<AccountBalance> {
    be.get_account(id)
}

pub fn find_by_name(be: &dyn LedgerBackend, name: &str) -> Result<Option<AccountBalance>> {
    be.find_account_by_name(name)
}

pub fn require_by_name(be: &dyn LedgerBackend, name: &str) -> Result<AccountBalance> {
    be.require_account_by_name(name)
}

pub fn emergency_account(be: &dyn LedgerBackend) -> Result<Option<AccountBalance>> {
    be.emergency_account()
}

/// Reads the `default_account` config key and resolves it to an account.
pub fn default_account(be: &dyn LedgerBackend) -> Result<AccountBalance> {
    let name = be
        .get_config("default_account")?
        .ok_or_else(|| {
            AppError::Config(
                "No default account set. Run: money-tracker config set default_account <name>"
                    .into(),
            )
        })?;
    require_by_name(be, &name)
}

pub fn set_default_account(be: &dyn LedgerBackend, name: &str) -> Result<()> {
    require_by_name(be, name)?;
    be.set_config("default_account", name)
}

/// Refuses to archive an account with a nonzero balance unless `force` is
/// passed — archiving otherwise hides money without anywhere for it to go.
pub fn archive_account(be: &dyn LedgerBackend, id: i64, force: bool) -> Result<()> {
    be.archive_account(id, force)
}

/// Balance derived from entries dated on or before `date` (inclusive).
pub fn balance_as_of(be: &dyn LedgerBackend, id: i64, date: &str) -> Result<f64> {
    be.balance_as_of(id, date)
}

/// The result of reconciling an account's derived balance against a
/// physically counted amount — see the "cash envelope" flow in the CLI.
pub struct ReconcileResult {
    pub entry_id: Option<i64>,
    pub diff: f64,
}

/// Writes the adjusting entry needed to bring an account's derived balance
/// to `actual`:
/// - `diff > 0` (derived > actual): an `expense` for the difference — the
///   ordinary case of untracked spending (e.g. the cash envelope).
/// - `diff < 0` (derived < actual): an `income` for the difference; `concept`
///   is required here since the tool cannot guess where unaccounted money
///   came from.
/// - `diff == 0`: no entry is written.
pub fn reconcile_account(
    be: &dyn LedgerBackend,
    account_id: i64,
    actual: f64,
    concept: &str,
    date: &str,
) -> Result<ReconcileResult> {
    let account = get_account(be, account_id)?;
    let diff = ((account.balance - actual) * 100.0).round() / 100.0;

    if diff.abs() < 0.005 {
        return Ok(ReconcileResult {
            entry_id: None,
            diff: 0.0,
        });
    }

    let entry = if diff > 0.0 {
        NewEntry::expense(date, diff, account_id, concept)?.with_description(Some("Cuadre de efectivo"))
    } else {
        NewEntry::income(date, -diff, account_id, concept)?.with_description(Some("Cuadre de efectivo"))
    };
    let inserted = be.push_entries(&[entry])?;

    Ok(ReconcileResult {
        entry_id: inserted.first().map(|e| e.id),
        diff,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite::SqliteBackend;

    fn setup() -> SqliteBackend {
        SqliteBackend::open_memory().unwrap()
    }

    #[test]
    fn create_and_list_accounts() {
        let be = setup();
        create_account(&be, &NewAccount::spending("efectivo")).unwrap();
        create_account(&be, &NewAccount::emergency("Fondo de emergencia")).unwrap();
        let accounts = list_accounts(&be, false).unwrap();
        assert_eq!(accounts.len(), 2);
    }

    #[test]
    fn second_emergency_account_is_rejected() {
        let be = setup();
        create_account(&be, &NewAccount::emergency("fondo")).unwrap();
        let err = create_account(&be, &NewAccount::emergency("fondo2"));
        assert!(err.is_err());
    }

    #[test]
    fn reconcile_deficit_writes_expense() {
        let be = setup();
        let id = create_account(&be, &NewAccount::spending("efectivo")).unwrap();
        crate::services::entry_service::add_income(&be, "2026-08-01", 1000.0, id, "Nomina", None)
            .unwrap();
        let result = reconcile_account(&be, id, 150.0, "Discrecional", "2026-08-31").unwrap();
        assert_eq!(result.diff, 850.0);
        assert!(result.entry_id.is_some());
        let account = get_account(&be, id).unwrap();
        assert_eq!(account.balance, 150.0);
    }

    #[test]
    fn reconcile_zero_diff_writes_nothing() {
        let be = setup();
        let id = create_account(&be, &NewAccount::spending("efectivo")).unwrap();
        crate::services::entry_service::add_income(&be, "2026-08-01", 1000.0, id, "Nomina", None)
            .unwrap();
        let result = reconcile_account(&be, id, 1000.0, "Discrecional", "2026-08-31").unwrap();
        assert_eq!(result.diff, 0.0);
        assert!(result.entry_id.is_none());
    }

    #[test]
    fn reconcile_surplus_writes_income() {
        let be = setup();
        let id = create_account(&be, &NewAccount::spending("efectivo")).unwrap();
        crate::services::entry_service::add_income(&be, "2026-08-01", 100.0, id, "Nomina", None)
            .unwrap();
        let result = reconcile_account(&be, id, 250.0, "Extra", "2026-08-31").unwrap();
        assert_eq!(result.diff, -150.0);
        let account = get_account(&be, id).unwrap();
        assert_eq!(account.balance, 250.0);
    }

    #[test]
    fn archive_refuses_nonzero_balance() {
        let be = setup();
        let id = create_account(&be, &NewAccount::spending("efectivo")).unwrap();
        crate::services::entry_service::add_income(&be, "2026-08-01", 100.0, id, "Nomina", None)
            .unwrap();
        assert!(archive_account(&be, id, false).is_err());
        assert!(archive_account(&be, id, true).is_ok());
    }
}