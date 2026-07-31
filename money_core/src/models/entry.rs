use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::period::validate_date;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts-rs", ts(rename_all = "lowercase"))]
pub enum EntryKind {
    Income,
    Expense,
    Transfer,
    /// Opening balance seeded via `setup`. Shape-identical to `income` but
    /// excluded from income totals — this is what keeps a seeded balance
    /// from inflating the month it was loaded in.
    Opening,
}

impl EntryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryKind::Income => "income",
            EntryKind::Expense => "expense",
            EntryKind::Transfer => "transfer",
            EntryKind::Opening => "opening",
        }
    }

    #[allow(clippy::should_implement_trait)] // inherent helper, not std::str::FromStr
    pub fn from_str(s: &str) -> Result<EntryKind> {
        match s {
            "income" => Ok(EntryKind::Income),
            "expense" => Ok(EntryKind::Expense),
            "transfer" => Ok(EntryKind::Transfer),
            "opening" => Ok(EntryKind::Opening),
            other => Err(AppError::Invalid(format!("Unknown entry kind: '{other}'"))),
        }
    }
}

/// Insert shape. The four constructors make an invalid endpoint combination
/// unrepresentable in Rust; the SQL CHECK constraint is a backstop, not the
/// primary guard.
#[derive(Debug, Clone)]
pub struct NewEntry {
    pub date: String,
    pub kind: EntryKind,
    pub amount: f64,
    pub from_account_id: Option<i64>,
    pub to_account_id: Option<i64>,
    pub concept: Option<String>,
    pub subconcept: Option<String>,
    pub description: Option<String>,
}

fn validate_amount(amount: f64) -> Result<()> {
    if amount <= 0.0 {
        return Err(AppError::Invalid("Amount must be positive".into()));
    }
    Ok(())
}

impl NewEntry {
    pub fn income(date: &str, amount: f64, to: i64, concept: &str) -> Result<NewEntry> {
        validate_date(date)?;
        validate_amount(amount)?;
        Ok(NewEntry {
            date: date.to_string(),
            kind: EntryKind::Income,
            amount,
            from_account_id: None,
            to_account_id: Some(to),
            concept: Some(concept.to_string()),
            subconcept: None,
            description: None,
        })
    }

    pub fn expense(date: &str, amount: f64, from: i64, concept: &str) -> Result<NewEntry> {
        validate_date(date)?;
        validate_amount(amount)?;
        Ok(NewEntry {
            date: date.to_string(),
            kind: EntryKind::Expense,
            amount,
            from_account_id: Some(from),
            to_account_id: None,
            concept: Some(concept.to_string()),
            subconcept: None,
            description: None,
        })
    }

    pub fn transfer(date: &str, amount: f64, from: i64, to: i64) -> Result<NewEntry> {
        validate_date(date)?;
        validate_amount(amount)?;
        if from == to {
            return Err(AppError::Invalid(
                "Transfer source and destination cannot be the same account".into(),
            ));
        }
        Ok(NewEntry {
            date: date.to_string(),
            kind: EntryKind::Transfer,
            amount,
            from_account_id: Some(from),
            to_account_id: Some(to),
            concept: None,
            subconcept: None,
            description: None,
        })
    }

    /// Opening balances have no concept: they aren't spending or income
    /// against a category, just the seed. Structurally excluded from
    /// income totals via `kind`, so no magic concept string is needed
    /// (unlike the old schema's `"Saldo inicial"`).
    pub fn opening(date: &str, amount: f64, to: i64) -> Result<NewEntry> {
        validate_date(date)?;
        validate_amount(amount)?;
        Ok(NewEntry {
            date: date.to_string(),
            kind: EntryKind::Opening,
            amount,
            from_account_id: None,
            to_account_id: Some(to),
            concept: None,
            subconcept: None,
            description: Some("Saldo inicial".to_string()),
        })
    }

    pub fn with_subconcept(mut self, s: Option<&str>) -> Self {
        self.subconcept = s.map(|s| s.to_string());
        self
    }

    pub fn with_description(mut self, d: Option<&str>) -> Self {
        self.description = d.map(|d| d.to_string());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct Entry {
    // See AccountBalance::id for why these override ts-rs's default i64 -> bigint mapping.
    #[cfg_attr(feature = "ts-rs", ts(type = "number"))]
    pub id: i64,
    pub date: String,
    pub kind: EntryKind,
    pub amount: f64,
    #[cfg_attr(feature = "ts-rs", ts(type = "number | null"))]
    pub from_account_id: Option<i64>,
    #[cfg_attr(feature = "ts-rs", ts(type = "number | null"))]
    pub to_account_id: Option<i64>,
    pub from_account: Option<String>,
    pub to_account: Option<String>,
    pub concept: Option<String>,
    pub subconcept: Option<String>,
    pub description: Option<String>,
}

impl Entry {
    /// Signed effect on a given account: +amount if it landed there,
    /// -amount if it left, 0 if the account wasn't involved.
    pub fn delta_for(&self, account_id: i64) -> f64 {
        let mut delta = 0.0;
        if self.to_account_id == Some(account_id) {
            delta += self.amount;
        }
        if self.from_account_id == Some(account_id) {
            delta -= self.amount;
        }
        delta
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct ConceptSummary {
    pub concept: String,
    pub total: f64,
    #[cfg_attr(feature = "ts-rs", ts(type = "number"))]
    pub count: i64,
}
