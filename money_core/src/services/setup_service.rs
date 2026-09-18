use crate::error::{AppError, Result};
use crate::models::NewEntry;
use crate::storage::LedgerBackend;

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
pub fn is_seeded(be: &dyn LedgerBackend) -> Result<bool> {
    be.is_seeded()
}

/// Writes opening-balance entries (`kind = 'opening'`, excluded from income
/// totals) for each account in `opts.accounts`. All-or-nothing via a single
/// atomic `push_entries` batch.
pub fn seed(be: &dyn LedgerBackend, opts: &SeedOptions) -> Result<SeedSummary> {
    if opts.accounts.is_empty() {
        return Err(AppError::Invalid("No opening balances given".into()));
    }

    let mut batch = Vec::new();
    let mut seeded = Vec::new();
    for (name, amount) in &opts.accounts {
        if *amount <= 0.0 {
            continue;
        }
        let account = be.require_account_by_name(name)?;
        batch.push(NewEntry::opening(&opts.date, *amount, account.id)?);
        seeded.push((name.clone(), *amount));
    }

    be.push_entries(&batch)?;
    Ok(SeedSummary { seeded })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NewAccount;
    use crate::period::Period;
    use crate::services::{account_service, report_service};
    use crate::storage::sqlite::SqliteBackend;

    fn setup_db() -> SqliteBackend {
        SqliteBackend::open_memory().unwrap()
    }

    #[test]
    fn seed_does_not_count_as_income() {
        let be = setup_db();
        account_service::create_account(&be, &NewAccount::spending("efectivo")).unwrap();
        account_service::create_account(&be, &NewAccount::emergency("fondo")).unwrap();

        assert!(!is_seeded(&be).unwrap());

        seed(
            &be,
            &SeedOptions {
                accounts: vec![("efectivo".into(), 1000.0), ("fondo".into(), 35000.0)],
                date: "2026-08-01".into(),
            },
        )
        .unwrap();

        assert!(is_seeded(&be).unwrap());

        let period = Period::parse("2026-08").unwrap();
        let report = report_service::monthly_report(&be, &period).unwrap();
        assert_eq!(report.total_income, 0.0);

        let efectivo = account_service::require_by_name(&be, "efectivo").unwrap();
        assert_eq!(efectivo.balance, 1000.0);
    }
}