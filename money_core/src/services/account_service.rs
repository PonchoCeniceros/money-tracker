use rusqlite::{Connection, OptionalExtension};

use crate::error::{AppError, Result};
use crate::models::{AccountBalance, AccountKind, NewAccount, NewEntry};
use crate::services::entry_service;

pub fn create_account(conn: &Connection, new: &NewAccount) -> Result<i64> {
    conn.execute(
        "INSERT INTO accounts (name, kind, target_amount, credit_limit, liquid)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            new.name,
            new.kind.as_str(),
            new.target_amount,
            new.credit_limit,
            new.liquid as i64,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_account_balance(row: &rusqlite::Row) -> rusqlite::Result<AccountBalance> {
    let kind_str: String = row.get(2)?;
    Ok(AccountBalance {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: AccountKind::from_str(&kind_str).unwrap_or(AccountKind::Spending),
        target_amount: row.get(3)?,
        credit_limit: row.get(4)?,
        liquid: row.get::<_, i64>(5)? == 1,
        archived: row.get::<_, i64>(6)? == 1,
        balance: row.get(7)?,
    })
}

const SELECT_ACCOUNT_BALANCES: &str =
    "SELECT id, name, kind, target_amount, credit_limit, liquid, archived, balance
       FROM account_balances";

pub fn list_accounts(conn: &Connection, include_archived: bool) -> Result<Vec<AccountBalance>> {
    let sql = if include_archived {
        format!("{SELECT_ACCOUNT_BALANCES} ORDER BY kind, name")
    } else {
        format!("{SELECT_ACCOUNT_BALANCES} WHERE archived = 0 ORDER BY kind, name")
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_account_balance)?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn get_account(conn: &Connection, id: i64) -> Result<AccountBalance> {
    conn.query_row(
        &format!("{SELECT_ACCOUNT_BALANCES} WHERE id = ?1"),
        rusqlite::params![id],
        row_to_account_balance,
    )
    .map_err(|_| AppError::NotFound(format!("Account #{id} not found")))
}

pub fn find_by_name(conn: &Connection, name: &str) -> Result<Option<AccountBalance>> {
    conn.query_row(
        &format!("{SELECT_ACCOUNT_BALANCES} WHERE name = ?1"),
        rusqlite::params![name],
        row_to_account_balance,
    )
    .optional()
    .map_err(AppError::from)
}

pub fn require_by_name(conn: &Connection, name: &str) -> Result<AccountBalance> {
    find_by_name(conn, name)?.ok_or_else(|| AppError::NotFound(format!("Account '{name}' not found")))
}

pub fn emergency_account(conn: &Connection) -> Result<Option<AccountBalance>> {
    conn.query_row(
        &format!("{SELECT_ACCOUNT_BALANCES} WHERE kind = 'emergency' AND archived = 0"),
        [],
        row_to_account_balance,
    )
    .optional()
    .map_err(AppError::from)
}

/// Reads the `default_account` config key and resolves it to an account.
pub fn default_account(conn: &Connection) -> Result<AccountBalance> {
    let name: String = conn
        .query_row(
            "SELECT value FROM config WHERE key = 'default_account'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| {
            AppError::Config(
                "No default account set. Run: money-tracker config set default_account <name>"
                    .into(),
            )
        })?;
    require_by_name(conn, &name)
}

pub fn set_default_account(conn: &Connection, name: &str) -> Result<()> {
    require_by_name(conn, name)?;
    conn.execute(
        "INSERT INTO config (key, value) VALUES ('default_account', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![name],
    )?;
    Ok(())
}

/// Refuses to archive an account with a nonzero balance unless `force` is
/// passed — archiving otherwise hides money without anywhere for it to go.
pub fn archive_account(conn: &Connection, id: i64, force: bool) -> Result<()> {
    let account = get_account(conn, id)?;
    if !force && account.balance.abs() > 0.005 {
        return Err(AppError::Invalid(format!(
            "'{}' has a balance of ${:.2}. Empty it first or pass --force",
            account.name, account.balance
        )));
    }
    conn.execute(
        "UPDATE accounts SET archived = 1 WHERE id = ?1",
        rusqlite::params![id],
    )?;
    Ok(())
}

/// Balance derived from entries dated on or before `date` (inclusive).
pub fn balance_as_of(conn: &Connection, id: i64, date: &str) -> Result<f64> {
    conn.query_row(
        "SELECT COALESCE(SUM(delta), 0.0) FROM (
            SELECT amount AS delta FROM entries
             WHERE to_account_id = ?1 AND date <= ?2
            UNION ALL
            SELECT -amount AS delta FROM entries
             WHERE from_account_id = ?1 AND date <= ?2
         )",
        rusqlite::params![id, date],
        |row| row.get(0),
    )
    .map_err(AppError::from)
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
    conn: &Connection,
    account_id: i64,
    actual: f64,
    concept: &str,
    date: &str,
) -> Result<ReconcileResult> {
    let account = get_account(conn, account_id)?;
    let diff = ((account.balance - actual) * 100.0).round() / 100.0;

    if diff.abs() < 0.005 {
        return Ok(ReconcileResult {
            entry_id: None,
            diff: 0.0,
        });
    }

    let entry_id = if diff > 0.0 {
        let entry = NewEntry::expense(date, diff, account_id, concept)?
            .with_description(Some("Cuadre de efectivo"));
        entry_service::insert(conn, &entry)?
    } else {
        let entry = NewEntry::income(date, -diff, account_id, concept)?
            .with_description(Some("Cuadre de efectivo"));
        entry_service::insert(conn, &entry)?
    };

    Ok(ReconcileResult {
        entry_id: Some(entry_id),
        diff,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        db::test_support::create_schema_for_tests(&conn);
        conn
    }

    #[test]
    fn create_and_list_accounts() {
        let conn = setup();
        create_account(&conn, &NewAccount::spending("efectivo")).unwrap();
        create_account(&conn, &NewAccount::emergency("Fondo de emergencia")).unwrap();
        let accounts = list_accounts(&conn, false).unwrap();
        assert_eq!(accounts.len(), 2);
    }

    #[test]
    fn second_emergency_account_is_rejected() {
        let conn = setup();
        create_account(&conn, &NewAccount::emergency("fondo")).unwrap();
        let err = create_account(&conn, &NewAccount::emergency("fondo2"));
        assert!(err.is_err());
    }

    #[test]
    fn reconcile_deficit_writes_expense() {
        let conn = setup();
        let id = create_account(&conn, &NewAccount::spending("efectivo")).unwrap();
        entry_service::add_income(&conn, "2026-08-01", 1000.0, id, "Nomina", None).unwrap();
        let result = reconcile_account(&conn, id, 150.0, "Discrecional", "2026-08-31").unwrap();
        assert_eq!(result.diff, 850.0);
        assert!(result.entry_id.is_some());
        let account = get_account(&conn, id).unwrap();
        assert_eq!(account.balance, 150.0);
    }

    #[test]
    fn reconcile_zero_diff_writes_nothing() {
        let conn = setup();
        let id = create_account(&conn, &NewAccount::spending("efectivo")).unwrap();
        entry_service::add_income(&conn, "2026-08-01", 1000.0, id, "Nomina", None).unwrap();
        let result = reconcile_account(&conn, id, 1000.0, "Discrecional", "2026-08-31").unwrap();
        assert_eq!(result.diff, 0.0);
        assert!(result.entry_id.is_none());
    }

    #[test]
    fn reconcile_surplus_writes_income() {
        let conn = setup();
        let id = create_account(&conn, &NewAccount::spending("efectivo")).unwrap();
        entry_service::add_income(&conn, "2026-08-01", 100.0, id, "Nomina", None).unwrap();
        let result = reconcile_account(&conn, id, 250.0, "Extra", "2026-08-31").unwrap();
        assert_eq!(result.diff, -150.0);
        let account = get_account(&conn, id).unwrap();
        assert_eq!(account.balance, 250.0);
    }

    #[test]
    fn archive_refuses_nonzero_balance() {
        let conn = setup();
        let id = create_account(&conn, &NewAccount::spending("efectivo")).unwrap();
        entry_service::add_income(&conn, "2026-08-01", 100.0, id, "Nomina", None).unwrap();
        assert!(archive_account(&conn, id, false).is_err());
        assert!(archive_account(&conn, id, true).is_ok());
    }
}
