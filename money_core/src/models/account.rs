use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts-rs", ts(rename_all = "lowercase"))]
pub enum AccountKind {
    Spending,
    Emergency,
    Target,
    Credit,
}

impl AccountKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountKind::Spending => "spending",
            AccountKind::Emergency => "emergency",
            AccountKind::Target => "target",
            AccountKind::Credit => "credit",
        }
    }

    #[allow(clippy::should_implement_trait)] // inherent helper, not std::str::FromStr
    pub fn from_str(s: &str) -> Result<AccountKind> {
        match s {
            "spending" => Ok(AccountKind::Spending),
            "emergency" => Ok(AccountKind::Emergency),
            "target" => Ok(AccountKind::Target),
            "credit" => Ok(AccountKind::Credit),
            other => Err(AppError::Invalid(format!("Unknown account kind: '{other}'"))),
        }
    }

    /// Whether this account represents an asset (vs. a liability like credit).
    pub fn is_asset(&self) -> bool {
        !matches!(self, AccountKind::Credit)
    }
}

/// Shape used to create a new account. Validated by the constructors below
/// so an invalid (kind, target_amount, credit_limit) combination cannot be
/// built in Rust; the DB CHECK constraints are a backstop.
#[derive(Debug, Clone)]
pub struct NewAccount {
    pub name: String,
    pub kind: AccountKind,
    pub target_amount: Option<f64>,
    pub credit_limit: Option<f64>,
    pub liquid: bool,
}

impl NewAccount {
    pub fn spending(name: &str) -> NewAccount {
        NewAccount {
            name: name.to_string(),
            kind: AccountKind::Spending,
            target_amount: None,
            credit_limit: None,
            liquid: true,
        }
    }

    pub fn emergency(name: &str) -> NewAccount {
        NewAccount {
            name: name.to_string(),
            kind: AccountKind::Emergency,
            target_amount: None,
            credit_limit: None,
            liquid: true,
        }
    }

    pub fn target(name: &str, target_amount: f64) -> Result<NewAccount> {
        if target_amount <= 0.0 {
            return Err(AppError::Invalid("Target amount must be positive".into()));
        }
        Ok(NewAccount {
            name: name.to_string(),
            kind: AccountKind::Target,
            target_amount: Some(target_amount),
            credit_limit: None,
            liquid: true,
        })
    }

    pub fn credit(name: &str, credit_limit: Option<f64>) -> NewAccount {
        NewAccount {
            name: name.to_string(),
            kind: AccountKind::Credit,
            target_amount: None,
            credit_limit,
            liquid: true,
        }
    }

    pub fn restricted(mut self) -> NewAccount {
        self.liquid = false;
        self
    }
}

/// The only shape ever read back for an account — always carries its
/// derived balance. There is deliberately no `Account` without a balance:
/// reading an account without its balance was how `buckets.current_balance`
/// went stale in the old schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct AccountBalance {
    // ts-rs maps i64 -> bigint by default, but Tauri's IPC serializes via
    // serde_json, which produces a plain JS `number` — override to match
    // what actually arrives at runtime.
    #[cfg_attr(feature = "ts-rs", ts(type = "number"))]
    pub id: i64,
    pub name: String,
    pub kind: AccountKind,
    pub target_amount: Option<f64>,
    pub credit_limit: Option<f64>,
    pub liquid: bool,
    pub archived: bool,
    /// Signed; negative on a credit account means debt.
    pub balance: f64,
}

impl AccountBalance {
    /// Percentage of the way to `target_amount`, clamped to 100. `None` for
    /// non-target accounts.
    pub fn progress_pct(&self) -> Option<f64> {
        self.target_amount.map(|t| {
            if t <= 0.0 {
                0.0
            } else {
                (self.balance / t * 100.0).clamp(0.0, 100.0)
            }
        })
    }

    /// Positive amount owed, or 0 if not a credit account or in credit.
    pub fn debt(&self) -> f64 {
        if self.kind == AccountKind::Credit {
            (-self.balance).max(0.0)
        } else {
            0.0
        }
    }

    pub fn available_credit(&self) -> Option<f64> {
        self.credit_limit.map(|limit| limit - self.debt())
    }

    pub fn is_asset(&self) -> bool {
        self.kind.is_asset()
    }
}
