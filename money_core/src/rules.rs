//! Accounting rules as pure functions: no storage, no I/O, no clock.
//!
//! Every write path in `services` calls these *before* touching a
//! `LedgerBackend`, so each rule exists exactly once and is testable without a
//! database. Supabase keeps its own CHECKs and `apply_entries` guard only as a
//! second line of defense (e.g. two devices writing at the same instant).

use crate::error::{AppError, Result};
use crate::models::{AccountBalance, AccountKind, Entry, EntryKind, EntryUpdate, NewAccount};
use crate::storage::round2;

/// Overdraft policy for the source of an expense or transfer: savings
/// buckets (`target`/`emergency`) can't go below zero, a credit card can't go
/// past its limit (debt = `max(-balance, 0)`), and spending accounts never
/// block.
pub fn check_source(account: &AccountBalance, amount: f64) -> Result<()> {
    match account.kind {
        AccountKind::Target | AccountKind::Emergency => {
            if amount > account.balance {
                return Err(AppError::Invalid(format!(
                    "Insufficient balance: have ${:.2}, need ${amount:.2}",
                    account.balance
                )));
            }
        }
        AccountKind::Credit => {
            if let Some(limit) = account.credit_limit {
                let debt = (-account.balance).max(0.0);
                if round2(debt + amount) > limit {
                    let available = round2(limit - debt);
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

/// At most one active emergency account.
pub fn check_new_emergency(existing_active: Option<&AccountBalance>) -> Result<()> {
    match existing_active {
        Some(fund) => Err(AppError::Invalid(format!(
            "There is already an active emergency account ('{}'). Archive it first",
            fund.name
        ))),
        None => Ok(()),
    }
}

/// Shape of a new account: `target_amount` only on `target`, `credit_limit`
/// only on `credit`, both positive when present.
pub fn validate_new_account(new: &NewAccount) -> Result<()> {
    if new.name.trim().is_empty() {
        return Err(AppError::Invalid("Account name cannot be empty".into()));
    }
    match new.target_amount {
        Some(_) if new.kind != AccountKind::Target => {
            return Err(AppError::Invalid(
                "Only target accounts can have a target amount".into(),
            ))
        }
        Some(t) if t <= 0.0 => {
            return Err(AppError::Invalid("Target amount must be positive".into()))
        }
        _ => {}
    }
    match new.credit_limit {
        Some(_) if new.kind != AccountKind::Credit => {
            return Err(AppError::Invalid(
                "Only credit accounts can have a credit limit".into(),
            ))
        }
        Some(l) if l <= 0.0 => {
            return Err(AppError::Invalid("Credit limit must be positive".into()))
        }
        _ => {}
    }
    Ok(())
}

pub fn validate_budget_limit(limit: f64) -> Result<()> {
    if limit <= 0.0 {
        return Err(AppError::Invalid("Limit must be positive".into()));
    }
    Ok(())
}

pub fn validate_concept_type(concept_type: &str) -> Result<()> {
    if !["expense", "income", "both"].contains(&concept_type) {
        return Err(AppError::Invalid(
            "Type must be expense, income, or both".into(),
        ));
    }
    Ok(())
}

/// Validates a correction to an existing entry and returns the complete
/// patch (every field that applies to the entry, already merged with its
/// current values), so a backend only has to write it. The kind never
/// changes: only the account side that already applies to it may move.
pub fn merge_entry_update(current: &Entry, upd: &EntryUpdate) -> Result<EntryUpdate> {
    match current.kind {
        EntryKind::Income | EntryKind::Opening => {
            if upd.from_account_id.is_some() {
                return Err(AppError::Invalid(
                    "This entry has no source account to change (it's an income/opening entry)"
                        .into(),
                ));
            }
        }
        EntryKind::Expense => {
            if upd.to_account_id.is_some() {
                return Err(AppError::Invalid(
                    "This entry has no destination account to change (it's an expense)".into(),
                ));
            }
        }
        EntryKind::Transfer => {}
    }
    if matches!(current.kind, EntryKind::Transfer | EntryKind::Opening) && upd.concept.is_some() {
        return Err(AppError::Invalid(
            "Transfers and opening balances don't carry a concept".into(),
        ));
    }

    let merged = EntryUpdate {
        date: Some(upd.date.clone().unwrap_or_else(|| current.date.clone())),
        amount: Some(upd.amount.unwrap_or(current.amount)),
        from_account_id: upd.from_account_id.or(current.from_account_id),
        to_account_id: upd.to_account_id.or(current.to_account_id),
        concept: upd.concept.clone().or_else(|| current.concept.clone()),
        subconcept: upd.subconcept.clone().or_else(|| current.subconcept.clone()),
        description: upd.description.clone().or_else(|| current.description.clone()),
    };

    crate::period::validate_date(merged.date.as_deref().unwrap_or_default())?;
    if merged.amount.unwrap_or(0.0) <= 0.0 {
        return Err(AppError::Invalid("Amount must be positive".into()));
    }
    if current.kind == EntryKind::Transfer && merged.from_account_id == merged.to_account_id {
        return Err(AppError::Invalid(
            "Transfer source and destination cannot be the same account".into(),
        ));
    }
    if matches!(current.kind, EntryKind::Income | EntryKind::Expense) && merged.concept.is_none() {
        return Err(AppError::Invalid("Concept is required".into()));
    }
    Ok(merged)
}

/// How much of an income is auto-split into the emergency fund, if any: only
/// when it lands in a liquid account, a fund is active, and the destination
/// isn't the fund itself. `pct` is the `emergency_pct` config value.
pub fn emergency_split(
    to: &AccountBalance,
    fund: Option<&AccountBalance>,
    pct: f64,
    amount: f64,
) -> Option<f64> {
    let fund = fund?;
    if !to.liquid || fund.id == to.id {
        return None;
    }
    let split = round2(amount * pct / 100.0);
    (split > 0.0).then_some(split)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: i64, kind: AccountKind, balance: f64, credit_limit: Option<f64>) -> AccountBalance {
        AccountBalance {
            id,
            name: format!("cuenta{id}"),
            kind,
            target_amount: None,
            credit_limit,
            liquid: true,
            archived: false,
            balance,
        }
    }

    fn entry(kind: EntryKind, from: Option<i64>, to: Option<i64>, concept: Option<&str>) -> Entry {
        Entry {
            id: 1,
            date: "2026-08-01".into(),
            kind,
            amount: 100.0,
            from_account_id: from,
            to_account_id: to,
            from_account: None,
            to_account: None,
            concept: concept.map(str::to_string),
            subconcept: None,
            description: None,
        }
    }

    // --- check_source -------------------------------------------------------

    #[test]
    fn bucket_withdrawal_over_balance_is_rejected() {
        let fondo = account(1, AccountKind::Emergency, 100.0, None);
        let err = check_source(&fondo, 100.01).unwrap_err();
        assert_eq!(err.to_string(), "Invalid input: Insufficient balance: have $100.00, need $100.01");
        check_source(&fondo, 100.0).unwrap();
        let meta = account(2, AccountKind::Target, 50.0, None);
        assert!(check_source(&meta, 60.0).is_err());
    }

    #[test]
    fn credit_limit_uses_current_debt() {
        // Production case (research R6): limit 3000, debt 1902.38 → 1097.62 available.
        let tdc = account(1, AccountKind::Credit, -1902.38, Some(3000.0));
        check_source(&tdc, 1097.62).unwrap();
        let err = check_source(&tdc, 1097.63).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Invalid input: Exceeds credit limit: $1097.62 available, $1097.63 requested"
        );
        assert!(check_source(&tdc, 2000.0).is_err());
    }

    #[test]
    fn credit_in_favor_does_not_add_room() {
        // A positive balance (overpaid card) is not extra credit: debt is 0, limit applies.
        let tdc = account(1, AccountKind::Credit, 500.0, Some(1000.0));
        check_source(&tdc, 1000.0).unwrap();
        assert!(check_source(&tdc, 1000.01).is_err());
    }

    #[test]
    fn credit_without_limit_and_spending_never_block() {
        check_source(&account(1, AccountKind::Credit, -99999.0, None), 5000.0).unwrap();
        check_source(&account(2, AccountKind::Spending, -500.0, None), 5000.0).unwrap();
    }

    // --- accounts ------------------------------------------------------------

    #[test]
    fn only_one_active_emergency_account() {
        check_new_emergency(None).unwrap();
        let existing = account(1, AccountKind::Emergency, 0.0, None);
        assert!(check_new_emergency(Some(&existing)).is_err());
    }

    #[test]
    fn target_amount_only_on_target_accounts() {
        let mut spending = NewAccount::spending("debito");
        validate_new_account(&spending).unwrap();
        spending.target_amount = Some(1000.0);
        assert!(validate_new_account(&spending).is_err());

        let open_bucket = NewAccount::target("Patrimonio", None).unwrap();
        validate_new_account(&open_bucket).unwrap();
    }

    #[test]
    fn credit_limit_only_on_credit_accounts_and_positive() {
        let mut debito = NewAccount::spending("debito");
        debito.credit_limit = Some(1000.0);
        assert!(validate_new_account(&debito).is_err());
        validate_new_account(&NewAccount::credit("tdc", Some(1000.0))).unwrap();
        assert!(validate_new_account(&NewAccount::credit("tdc", Some(0.0))).is_err());
        assert!(validate_new_account(&NewAccount::spending("  ")).is_err());
    }

    // --- budgets / concepts ----------------------------------------------------

    #[test]
    fn budget_limit_must_be_positive() {
        validate_budget_limit(2500.0).unwrap();
        assert_eq!(
            validate_budget_limit(0.0).unwrap_err().to_string(),
            "Invalid input: Limit must be positive"
        );
        assert!(validate_budget_limit(-1.0).is_err());
    }

    #[test]
    fn concept_type_whitelist() {
        for ok in ["expense", "income", "both"] {
            validate_concept_type(ok).unwrap();
        }
        assert_eq!(
            validate_concept_type("gasto").unwrap_err().to_string(),
            "Invalid input: Type must be expense, income, or both"
        );
    }

    // --- entry updates ----------------------------------------------------------

    #[test]
    fn update_rejects_the_wrong_side_for_the_kind() {
        let expense = entry(EntryKind::Expense, Some(1), None, Some("Alimentos"));
        let upd = EntryUpdate { to_account_id: Some(2), ..Default::default() };
        assert!(merge_entry_update(&expense, &upd).is_err());

        let income = entry(EntryKind::Income, None, Some(1), Some("Nomina"));
        let upd = EntryUpdate { from_account_id: Some(2), ..Default::default() };
        assert!(merge_entry_update(&income, &upd).is_err());
    }

    #[test]
    fn update_rejects_self_transfer_bad_amount_bad_date_and_concept_on_transfer() {
        let transfer = entry(EntryKind::Transfer, Some(1), Some(2), None);
        let to_self = EntryUpdate { to_account_id: Some(1), ..Default::default() };
        assert!(merge_entry_update(&transfer, &to_self).is_err());
        let concept = EntryUpdate { concept: Some("Alimentos".into()), ..Default::default() };
        assert!(merge_entry_update(&transfer, &concept).is_err());

        let expense = entry(EntryKind::Expense, Some(1), None, Some("Alimentos"));
        let zero = EntryUpdate { amount: Some(0.0), ..Default::default() };
        assert!(merge_entry_update(&expense, &zero).is_err());
        let bad_date = EntryUpdate { date: Some("2026-8-1".into()), ..Default::default() };
        assert!(merge_entry_update(&expense, &bad_date).is_err());
    }

    #[test]
    fn update_merges_into_a_complete_patch() {
        let expense = entry(EntryKind::Expense, Some(1), None, Some("Alimentos"));
        let upd = EntryUpdate { amount: Some(177.0), from_account_id: Some(3), ..Default::default() };
        let merged = merge_entry_update(&expense, &upd).unwrap();
        assert_eq!(merged.amount, Some(177.0));
        assert_eq!(merged.from_account_id, Some(3));
        assert_eq!(merged.to_account_id, None);
        assert_eq!(merged.date.as_deref(), Some("2026-08-01"));
        assert_eq!(merged.concept.as_deref(), Some("Alimentos"));
    }

    // --- emergency split ----------------------------------------------------------

    #[test]
    fn split_applies_only_to_liquid_destination_with_an_active_fund() {
        let debito = account(1, AccountKind::Spending, 0.0, None);
        let fondo = account(2, AccountKind::Emergency, 0.0, None);
        assert_eq!(emergency_split(&debito, Some(&fondo), 10.0, 24000.0), Some(2400.0));
        assert_eq!(emergency_split(&debito, None, 10.0, 24000.0), None);

        let mut vales = account(3, AccountKind::Spending, 0.0, None);
        vales.liquid = false;
        assert_eq!(emergency_split(&vales, Some(&fondo), 10.0, 2400.0), None);

        // Income straight into the fund itself is never split into itself.
        assert_eq!(emergency_split(&fondo, Some(&fondo), 10.0, 1000.0), None);
        // Rounds to cents; a zero split is no split.
        assert_eq!(emergency_split(&debito, Some(&fondo), 10.0, 0.04), None);
        assert_eq!(emergency_split(&debito, Some(&fondo), 12.5, 333.33), Some(41.67));
    }
}
