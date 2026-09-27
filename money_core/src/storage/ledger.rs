//! Pure money math. Nothing here touches storage, time, or I/O — every
//! function maps `[Account] + [Entry]` (and optional `[Budget]`) into the
//! report shapes, so the SQLite and Supabase backends produce identical
//! numbers by construction (Principle III: the ledger is the source of
//! truth, and it now lives here, not in SQL text).

use std::collections::HashMap;

use crate::models::{Account, AccountBalance, Budget, ConceptSummary, Entry, EntryKind};
use crate::period::Period;
use crate::services::report_service::{BudgetVsActual, MonthlyReport, NetWorth};
use crate::storage::round2;

/// Derives every account's balance from the entry list. `entries` must
/// already be scoped (all entries, or only those `<= up_to_date`).
pub fn derive_balances(accounts: &[Account], entries: &[Entry]) -> Vec<AccountBalance> {
    let mut deltas: HashMap<i64, f64> = HashMap::new();
    for e in entries {
        if let Some(to) = e.to_account_id {
            *deltas.entry(to).or_insert(0.0) += e.amount;
        }
        if let Some(from) = e.from_account_id {
            *deltas.entry(from).or_insert(0.0) -= e.amount;
        }
    }
    accounts
        .iter()
        .map(|a| AccountBalance {
            id: a.id,
            name: a.name.clone(),
            kind: a.kind,
            target_amount: a.target_amount,
            credit_limit: a.credit_limit,
            liquid: a.liquid,
            archived: a.archived,
            balance: round2(deltas.get(&a.id).copied().unwrap_or(0.0)),
        })
        .collect()
}

/// id -> kind, used to classify funding source/flow below.
fn kind_of(accounts: &[Account]) -> HashMap<i64, String> {
    accounts
        .iter()
        .map(|a| (a.id, a.kind.as_str().to_string()))
        .collect()
}

fn in_period(e: &Entry, start: &str, end: &str) -> bool {
    e.date.as_str() >= start && e.date.as_str() < end
}

pub fn monthly_report(
    period: &Period,
    accounts: &[Account],
    entries: &[Entry],
    budgets: &[Budget],
) -> MonthlyReport {
    let start = period.start();
    let end = period.end_exclusive();
    let kinds = kind_of(accounts);

    let total_income: f64 = entries
        .iter()
        .filter(|e| in_period(e, &start, &end) && e.kind == EntryKind::Income)
        .map(|e| e.amount)
        .sum();

    let expenses: Vec<&Entry> = entries
        .iter()
        .filter(|e| in_period(e, &start, &end) && e.kind == EntryKind::Expense)
        .collect();

    let total_expense: f64 = expenses.iter().map(|e| e.amount).sum();

    let expense_cash: f64 = expenses
        .iter()
        .filter(|e| {
            e.from_account_id
                .and_then(|id| kinds.get(&id))
                .map(|k| k == "spending")
                .unwrap_or(false)
        })
        .map(|e| e.amount)
        .sum();

    let transfers: Vec<&Entry> = entries
        .iter()
        .filter(|e| in_period(e, &start, &end) && e.kind == EntryKind::Transfer)
        .collect();

    let card_payments: f64 = transfers
        .iter()
        .filter(|e| {
            matches!(
                (
                    e.from_account_id.and_then(|id| kinds.get(&id)),
                    e.to_account_id.and_then(|id| kinds.get(&id)),
                ),
                (Some(from), Some(to)) if *from == "spending" && *to == "credit"
            )
        })
        .map(|e| e.amount)
        .sum();

    let cash_out = round2(expense_cash + card_payments);

    let from_savings: f64 = expenses
        .iter()
        .filter(|e| {
            e.from_account_id
                .and_then(|id| kinds.get(&id))
                .map(|k| k == "emergency" || k == "target")
                .unwrap_or(false)
        })
        .map(|e| e.amount)
        .sum();

    let on_credit: f64 = expenses
        .iter()
        .filter(|e| {
            e.from_account_id
                .and_then(|id| kinds.get(&id))
                .map(|k| k == "credit")
                .unwrap_or(false)
        })
        .map(|e| e.amount)
        .sum();

    let savings_contributions: f64 = transfers
        .iter()
        .filter(|e| {
            matches!(
                (
                    e.from_account_id.and_then(|id| kinds.get(&id)),
                    e.to_account_id.and_then(|id| kinds.get(&id)),
                ),
                (Some(from), Some(to)) if *from == "spending" && (to == "emergency" || to == "target")
            )
        })
        .map(|e| e.amount)
        .sum();

    let savings_withdrawals: f64 = transfers
        .iter()
        .filter(|e| {
            matches!(
                (
                    e.from_account_id.and_then(|id| kinds.get(&id)),
                    e.to_account_id.and_then(|id| kinds.get(&id)),
                ),
                (Some(from), Some(to)) if (from == "emergency" || from == "target") && *to == "spending"
            )
        })
        .map(|e| e.amount)
        .sum();

    let mut concept_totals: HashMap<&str, (f64, i64)> = HashMap::new();
    for e in &expenses {
        let ent = concept_totals
            .entry(e.concept.as_deref().unwrap_or(""))
            .or_insert((0.0, 0));
        ent.0 += e.amount;
        ent.1 += 1;
    }
    let mut by_concept: Vec<ConceptSummary> = concept_totals
        .into_iter()
        .map(|(concept, (total, count))| ConceptSummary {
            concept: concept.to_string(),
            total: round2(total),
            count,
        })
        .collect();
    by_concept.sort_by(|a, b| b.total.total_cmp(&a.total));

    let mut spent_by_concept: HashMap<&str, f64> = HashMap::new();
    for e in &expenses {
        *spent_by_concept
            .entry(e.concept.as_deref().unwrap_or(""))
            .or_insert(0.0) += e.amount;
    }
    let mut budgets: Vec<BudgetVsActual> = budgets
        .iter()
        .filter(|b| b.period == period.as_str())
        .map(|b| {
            let actual = round2(spent_by_concept.get(b.concept.as_str()).copied().unwrap_or(0.0));
            BudgetVsActual {
                concept: b.concept.clone(),
                budgeted: b.monthly_limit,
                actual,
                pct: if b.monthly_limit > 0.0 {
                    (actual / b.monthly_limit * 100.0).min(999.0)
                } else {
                    0.0
                },
            }
        })
        .collect();
    budgets.sort_by(|a, b| a.concept.cmp(&b.concept));

    MonthlyReport {
        period: period.clone(),
        total_income: round2(total_income),
        total_expense: round2(total_expense),
        cash_out,
        net_flow: round2(total_income - total_expense),
        by_concept,
        budgets,
        from_savings: round2(from_savings),
        on_credit: round2(on_credit),
        savings_contributions: round2(savings_contributions),
        savings_withdrawals: round2(savings_withdrawals),
        card_payments: round2(card_payments),
    }
}

pub fn net_worth(accounts: &[AccountBalance]) -> NetWorth {
    let cash_on_hand: f64 = accounts
        .iter()
        .filter(|a| a.kind.as_str() == "spending")
        .map(|a| a.balance)
        .sum();
    let savings: f64 = accounts
        .iter()
        .filter(|a| {
            a.kind.as_str() == "emergency" || a.kind.as_str() == "target"
        })
        .map(|a| a.balance)
        .sum();
    let credit_debt: f64 = accounts.iter().map(|a| a.debt()).sum();
    NetWorth {
        accounts: accounts.to_vec(),
        cash_on_hand: round2(cash_on_hand),
        savings: round2(savings),
        credit_debt: round2(credit_debt),
        net: round2(cash_on_hand + savings - credit_debt),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AccountKind;

    fn account(id: i64, name: &str, kind: AccountKind) -> Account {
        Account {
            id,
            name: name.to_string(),
            kind,
            target_amount: None,
            credit_limit: None,
            liquid: true,
            archived: false,
        }
    }

    fn income(id: i64, date: &str, amount: f64, to: i64) -> Entry {
        Entry {
            id,
            date: date.to_string(),
            kind: EntryKind::Income,
            amount,
            from_account_id: None,
            to_account_id: Some(to),
            from_account: None,
            to_account: None,
            concept: Some("Nomina".into()),
            subconcept: None,
            description: None,
        }
    }

    fn expense(id: i64, date: &str, amount: f64, from: i64, concept: &str) -> Entry {
        Entry {
            id,
            date: date.to_string(),
            kind: EntryKind::Expense,
            amount,
            from_account_id: Some(from),
            to_account_id: None,
            from_account: None,
            to_account: None,
            concept: Some(concept.into()),
            subconcept: None,
            description: None,
        }
    }

    #[test]
    fn derive_sums_incoming_and_outgoing_per_account() {
        let accounts = vec![account(1, "debito", AccountKind::Spending)];
        let entries = vec![
            income(1, "2026-08-01", 1000.0, 1),
            expense(2, "2026-08-02", 300.0, 1, "Alimentos"),
        ];
        let balances = derive_balances(&accounts, &entries);
        assert_eq!(balances[0].balance, 700.0);
    }

    #[test]
    fn monthly_report_rounds_and_classifies() {
        let accounts = vec![
            account(1, "debito", AccountKind::Spending),
            account(2, "tdc", AccountKind::Credit),
        ];
        let entries = vec![
            income(1, "2026-08-01", 10000.0, 1),
            expense(2, "2026-08-12", 1800.0, 2, "Discrecional"),
        ];
        let report = monthly_report(
            &Period::parse("2026-08").unwrap(),
            &accounts,
            &entries,
            &[],
        );
        assert_eq!(report.total_income, 10000.0);
        assert_eq!(report.total_expense, 1800.0);
        assert_eq!(report.on_credit, 1800.0);
        assert_eq!(report.cash_out, 0.0);
    }

    #[test]
    fn budget_join_over_period() {
        let accounts = vec![account(1, "debito", AccountKind::Spending)];
        let entries = vec![expense(1, "2026-08-05", 2000.0, 1, "Alimentos")];
        let budgets = vec![Budget {
            id: None,
            concept: "Alimentos".into(),
            monthly_limit: 2500.0,
            period: "2026-08".into(),
        }];
        let report = monthly_report(
            &Period::parse("2026-08").unwrap(),
            &accounts,
            &entries,
            &budgets,
        );
        assert_eq!(report.budgets.len(), 1);
        assert_eq!(report.budgets[0].actual, 2000.0);
        assert_eq!(report.budgets[0].pct, 80.0);
    }
}