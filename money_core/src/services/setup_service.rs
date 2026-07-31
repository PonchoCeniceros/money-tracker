use rusqlite::Connection;

use crate::error::{AppError, Result};
use crate::services::{account_service, entry_service};

pub struct SeedOptions {
    /// (account name, opening balance) pairs.
    pub accounts: Vec<(String, f64)>,
    pub date: String,
}

pub struct SeedSummary {
    /// (account name, amount seeded) in the order applied.
    pub seeded: Vec<(String, f64)>,
}

/// Whether the database already has any entries. `setup` refuses to run
/// again on a non-fresh DB unless `--force` overrides this at the CLI layer.
pub fn is_seeded(conn: &Connection) -> Result<bool> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
    Ok(count > 0)
}

/// Writes opening-balance entries (`kind = 'opening'`, excluded from income
/// totals) for each account in `opts.accounts`. All-or-nothing.
pub fn seed(conn: &mut Connection, opts: &SeedOptions) -> Result<SeedSummary> {
    if opts.accounts.is_empty() {
        return Err(AppError::Invalid("No opening balances given".into()));
    }

    let tx = conn.transaction()?;
    let mut seeded = Vec::new();
    for (name, amount) in &opts.accounts {
        if *amount <= 0.0 {
            continue;
        }
        let account = account_service::require_by_name(&tx, name)?;
        entry_service::add_opening(&tx, &opts.date, *amount, account.id)?;
        seeded.push((name.clone(), *amount));
    }
    tx.commit()?;

    Ok(SeedSummary { seeded })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::NewAccount;
    use crate::services::report_service;
    use crate::period::Period;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        db::test_support::create_schema_for_tests(&conn);
        conn
    }

    #[test]
    fn seed_does_not_count_as_income() {
        let mut conn = setup_db();
        account_service::create_account(&conn, &NewAccount::spending("efectivo")).unwrap();
        account_service::create_account(&conn, &NewAccount::emergency("fondo")).unwrap();

        assert!(!is_seeded(&conn).unwrap());

        seed(
            &mut conn,
            &SeedOptions {
                accounts: vec![("efectivo".into(), 1000.0), ("fondo".into(), 35000.0)],
                date: "2026-08-01".into(),
            },
        )
        .unwrap();

        assert!(is_seeded(&conn).unwrap());

        let period = Period::parse("2026-08").unwrap();
        let report = report_service::monthly_report(&conn, &period).unwrap();
        assert_eq!(report.total_income, 0.0);

        let efectivo = account_service::require_by_name(&conn, "efectivo").unwrap();
        assert_eq!(efectivo.balance, 1000.0);
    }
}
