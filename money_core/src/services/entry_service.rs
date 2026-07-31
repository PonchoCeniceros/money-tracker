use rusqlite::{Connection, OptionalExtension};

use crate::error::{AppError, Result};
use crate::models::{AccountKind, Entry, EntryKind, NewEntry};
use crate::period::Period;

/// Sole writer of `entries`. Every other constructor in this module funnels
/// through here, so the invariants encoded in `NewEntry`'s constructors are
/// the only path onto disk.
pub fn insert(conn: &Connection, e: &NewEntry) -> Result<i64> {
    conn.execute(
        "INSERT INTO entries (date, kind, amount, from_account_id, to_account_id,
                               concept, subconcept, description)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            e.date,
            e.kind.as_str(),
            e.amount,
            e.from_account_id,
            e.to_account_id,
            e.concept,
            e.subconcept,
            e.description,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn add_income(
    conn: &Connection,
    date: &str,
    amount: f64,
    to: i64,
    concept: &str,
    description: Option<&str>,
) -> Result<i64> {
    let entry = NewEntry::income(date, amount, to, concept)?.with_description(description);
    insert(conn, &entry)
}

pub fn add_expense(
    conn: &Connection,
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
    with_checked_source(conn, from, amount, |conn| insert(conn, &entry))
}

pub fn add_transfer(
    conn: &Connection,
    date: &str,
    amount: f64,
    from: i64,
    to: i64,
    description: Option<&str>,
) -> Result<i64> {
    let entry = NewEntry::transfer(date, amount, from, to)?.with_description(description);
    with_checked_source(conn, from, amount, |conn| insert(conn, &entry))
}

pub fn add_opening(conn: &Connection, date: &str, amount: f64, to: i64) -> Result<i64> {
    let entry = NewEntry::opening(date, amount, to)?;
    insert(conn, &entry)
}

pub struct IncomeResult {
    pub entry_id: i64,
    /// (account name, amount) transferred to the emergency fund, if any.
    pub emergency: Option<(String, f64)>,
}

/// Registers an income and, if the destination account is liquid and an
/// emergency account exists, auto-splits `emergency_pct`% into it.
///
/// Takes `&mut Connection` because this is the one genuinely multi-statement
/// write in the crate — the `&mut` is the honest signal of that, rather than
/// resurrecting the `execute_batch("BEGIN TRANSACTION")` pattern the old
/// importer used (not RAII: any `?` in between leaves the connection with
/// an open transaction).
pub fn add_income_with_emergency_split(
    conn: &mut Connection,
    date: &str,
    amount: f64,
    to: i64,
    concept: &str,
    description: Option<&str>,
    split: bool,
) -> Result<IncomeResult> {
    let tx = conn.transaction()?;

    let entry = NewEntry::income(date, amount, to, concept)?.with_description(description);
    let entry_id = insert(&tx, &entry)?;

    let mut emergency = None;
    if split {
        let to_liquid: bool = tx.query_row(
            "SELECT liquid FROM accounts WHERE id = ?1",
            rusqlite::params![to],
            |row| row.get::<_, i64>(0),
        )? == 1;

        if to_liquid {
            let emergency_account: Option<(i64, String)> = tx
                .query_row(
                    "SELECT id, name FROM accounts WHERE kind = 'emergency' AND archived = 0",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            if let Some((fund_id, fund_name)) = emergency_account {
                if fund_id != to {
                    let pct: f64 = tx
                        .query_row(
                            "SELECT value FROM config WHERE key = 'emergency_pct'",
                            [],
                            |row| row.get::<_, String>(0),
                        )
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(10.0);
                    let split_amount = (amount * pct / 100.0 * 100.0).round() / 100.0;
                    if split_amount > 0.0 {
                        let transfer = NewEntry::transfer(date, split_amount, to, fund_id)?;
                        insert(&tx, &transfer)?;
                        emergency = Some((fund_name, split_amount));
                    }
                }
            }
        }
    }

    tx.commit()?;
    Ok(IncomeResult {
        entry_id,
        emergency,
    })
}

/// Runs `write` inside the same IMMEDIATE transaction as the overdraft
/// check on `account_id`, so nothing can race between reading the balance
/// and writing the entry that depends on it (the check-then-act bug the old
/// `bucket_service` had). `Connection` methods take `&self` — sqlite manages
/// its own locking — so a plain `BEGIN IMMEDIATE` / `COMMIT` pair works here
/// without needing `&mut Connection` or `rusqlite::Transaction`.
///
/// Overdraft policy for the source of an expense or transfer:
/// - `target`/`emergency`: hard error if the withdrawal exceeds the balance
/// - `credit`: hard error if it would exceed `credit_limit` (when set)
/// - `spending`: no check — the balance is allowed to go negative, since the
///   user may simply not have recorded income yet
fn with_checked_source<T>(
    conn: &Connection,
    account_id: i64,
    amount: f64,
    write: impl FnOnce(&Connection) -> Result<T>,
) -> Result<T> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = check_source_locked(conn, account_id, amount).and_then(|()| write(conn));
    match &result {
        Ok(_) => conn.execute_batch("COMMIT")?,
        Err(_) => conn.execute_batch("ROLLBACK")?,
    }
    result
}

fn check_source_locked(conn: &Connection, account_id: i64, amount: f64) -> Result<()> {
    let (kind_str, balance, credit_limit): (String, f64, Option<f64>) = conn.query_row(
        "SELECT kind, balance, credit_limit FROM account_balances WHERE id = ?1",
        rusqlite::params![account_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let kind = AccountKind::from_str(&kind_str)?;

    match kind {
        AccountKind::Target | AccountKind::Emergency => {
            if amount > balance {
                return Err(AppError::Invalid(format!(
                    "Insufficient balance: have ${balance:.2}, need ${amount:.2}"
                )));
            }
        }
        AccountKind::Credit => {
            if let Some(limit) = credit_limit {
                let debt = (-balance).max(0.0);
                if debt + amount > limit {
                    let available = limit - debt;
                    return Err(AppError::Invalid(format!(
                        "Exceeds credit limit: ${available:.2} available, ${amount:.2} requested"
                    )));
                }
            }
        }
        AccountKind::Spending => {}
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct EntryFilter {
    pub period: Option<Period>,
    pub kind: Option<EntryKind>,
    pub concept: Option<String>,
    pub account_id: Option<i64>,
    pub limit: Option<u32>,
}

pub fn list(conn: &Connection, f: &EntryFilter) -> Result<Vec<Entry>> {
    let mut sql = String::from(
        "SELECT id, date, kind, amount, from_account_id, to_account_id,
                from_account, to_account, concept, subconcept, description
           FROM entries_view WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(period) = &f.period {
        sql.push_str(" AND date >= ? AND date < ?");
        params.push(Box::new(period.start()));
        params.push(Box::new(period.end_exclusive()));
    }
    if let Some(kind) = f.kind {
        sql.push_str(" AND kind = ?");
        params.push(Box::new(kind.as_str().to_string()));
    }
    if let Some(concept) = &f.concept {
        sql.push_str(" AND concept = ?");
        params.push(Box::new(concept.clone()));
    }
    if let Some(account_id) = f.account_id {
        sql.push_str(" AND (from_account_id = ? OR to_account_id = ?)");
        params.push(Box::new(account_id));
        params.push(Box::new(account_id));
    }
    sql.push_str(" ORDER BY date DESC, id DESC");
    if let Some(limit) = f.limit {
        sql.push_str(&format!(" LIMIT {limit}"));
    }

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let kind_str: String = row.get(2)?;
        Ok(Entry {
            id: row.get(0)?,
            date: row.get(1)?,
            kind: EntryKind::from_str(&kind_str).unwrap_or(EntryKind::Expense),
            amount: row.get(3)?,
            from_account_id: row.get(4)?,
            to_account_id: row.get(5)?,
            from_account: row.get(6)?,
            to_account: row.get(7)?,
            concept: row.get(8)?,
            subconcept: row.get(9)?,
            description: row.get(10)?,
        })
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let affected = conn.execute("DELETE FROM entries WHERE id = ?1", rusqlite::params![id])?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("Entry #{id} not found")));
    }
    Ok(())
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

    fn make_account(conn: &Connection, name: &str, kind: &str) -> i64 {
        conn.execute(
            "INSERT INTO accounts (name, kind) VALUES (?1, ?2)",
            rusqlite::params![name, kind],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn expense_and_income_move_balance() {
        let conn = setup();
        let debito = make_account(&conn, "debito", "spending");
        add_income(&conn, "2026-08-01", 1000.0, debito, "Nomina", None).unwrap();
        add_expense(&conn, "2026-08-02", 300.0, debito, "Alimentos", None, None).unwrap();
        let balance: f64 = conn
            .query_row(
                "SELECT balance FROM account_balances WHERE id = ?1",
                rusqlite::params![debito],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(balance, 700.0);
    }

    #[test]
    fn transfer_does_not_change_total_across_two_accounts() {
        let conn = setup();
        let debito = make_account(&conn, "debito", "spending");
        let fondo = make_account(&conn, "fondo", "emergency");
        add_income(&conn, "2026-08-01", 1000.0, debito, "Nomina", None).unwrap();
        add_transfer(&conn, "2026-08-02", 200.0, debito, fondo, None).unwrap();

        let deb: f64 = conn
            .query_row(
                "SELECT balance FROM account_balances WHERE id = ?1",
                rusqlite::params![debito],
                |r| r.get(0),
            )
            .unwrap();
        let fon: f64 = conn
            .query_row(
                "SELECT balance FROM account_balances WHERE id = ?1",
                rusqlite::params![fondo],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deb, 800.0);
        assert_eq!(fon, 200.0);
    }

    #[test]
    fn emergency_withdrawal_over_balance_is_rejected() {
        let conn = setup();
        let debito = make_account(&conn, "debito", "spending");
        let fondo = make_account(&conn, "fondo", "emergency");
        add_opening(&conn, "2026-08-01", 100.0, fondo).unwrap();
        let err = add_transfer(&conn, "2026-08-02", 500.0, fondo, debito, None);
        assert!(err.is_err());
    }

    #[test]
    fn self_transfer_is_rejected() {
        let conn = setup();
        let debito = make_account(&conn, "debito", "spending");
        add_income(&conn, "2026-08-01", 100.0, debito, "Nomina", None).unwrap();
        let err = add_transfer(&conn, "2026-08-02", 50.0, debito, debito, None);
        assert!(err.is_err());
    }

    #[test]
    fn credit_charge_over_limit_is_rejected() {
        let conn = setup();
        let tdc = make_account(&conn, "tdc", "credit");
        conn.execute(
            "UPDATE accounts SET credit_limit = 1000 WHERE id = ?1",
            rusqlite::params![tdc],
        )
        .unwrap();
        let err = add_expense(&conn, "2026-08-02", 1500.0, tdc, "Discrecional", None, None);
        assert!(err.is_err());
        // under the limit still works
        add_expense(&conn, "2026-08-03", 500.0, tdc, "Discrecional", None, None).unwrap();
    }

    #[test]
    fn credit_card_charge_and_payment_net_to_zero() {
        let conn = setup();
        let debito = make_account(&conn, "debito", "spending");
        let tdc = make_account(&conn, "tdc", "credit");
        add_income(&conn, "2026-08-01", 5000.0, debito, "Nomina", None).unwrap();
        add_expense(&conn, "2026-08-12", 1800.0, tdc, "Discrecional", None, None).unwrap();
        add_transfer(&conn, "2026-09-05", 1800.0, debito, tdc, None).unwrap();

        let tdc_balance: f64 = conn
            .query_row(
                "SELECT balance FROM account_balances WHERE id = ?1",
                rusqlite::params![tdc],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tdc_balance, 0.0);

        // no double counting: total expense across both months is 1800, not 3600
        let total_expense: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(amount),0) FROM entries WHERE kind = 'expense'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total_expense, 1800.0);
    }

    #[test]
    fn opening_is_excluded_from_income_totals() {
        let conn = setup();
        let debito = make_account(&conn, "debito", "spending");
        add_opening(&conn, "2026-08-01", 5000.0, debito).unwrap();
        let income_total: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(amount),0) FROM entries WHERE kind = 'income'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(income_total, 0.0);
        let balance: f64 = conn
            .query_row(
                "SELECT balance FROM account_balances WHERE id = ?1",
                rusqlite::params![debito],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(balance, 5000.0);
    }

    #[test]
    fn emergency_split_skipped_for_restricted_account() {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (name, kind, liquid) VALUES ('vales', 'spending', 0)",
            [],
        )
        .unwrap();
        let vales = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO accounts (name, kind) VALUES ('fondo', 'emergency')",
            [],
        )
        .unwrap();
        let fondo = conn.last_insert_rowid();

        let mut conn = conn;
        let result = add_income_with_emergency_split(
            &mut conn,
            "2026-08-01",
            2400.0,
            vales,
            "Vales de Despensa",
            None,
            true,
        )
        .unwrap();
        assert!(result.emergency.is_none());

        let fondo_balance: f64 = conn
            .query_row(
                "SELECT balance FROM account_balances WHERE id = ?1",
                rusqlite::params![fondo],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fondo_balance, 0.0);
    }

    #[test]
    fn emergency_split_applies_for_liquid_account() {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (name, kind) VALUES ('debito', 'spending')",
            [],
        )
        .unwrap();
        let debito = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO accounts (name, kind) VALUES ('fondo', 'emergency')",
            [],
        )
        .unwrap();
        let fondo = conn.last_insert_rowid();

        let mut conn = conn;
        let result = add_income_with_emergency_split(
            &mut conn, "2026-08-01", 24000.0, debito, "Nomina", None, true,
        )
        .unwrap();
        assert_eq!(result.emergency, Some(("fondo".to_string(), 2400.0)));

        let fondo_balance: f64 = conn
            .query_row(
                "SELECT balance FROM account_balances WHERE id = ?1",
                rusqlite::params![fondo],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fondo_balance, 2400.0);
    }
}
