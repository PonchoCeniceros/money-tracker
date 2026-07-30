use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: Option<i64>,
    pub date: String,
    pub amount: f64,
    pub concept: String,
    pub subconcept: Option<String>,
    pub tipo: Option<String>,
    pub description: Option<String>,
    pub month: i32,
    pub year: i32,
}

#[derive(Debug, Clone)]
pub struct TransactionSummary {
    pub concept: String,
    pub total: f64,
    pub count: i64,
}

impl Transaction {
    pub fn is_income(&self) -> bool {
        self.amount > 0.0
    }

    pub fn is_expense(&self) -> bool {
        self.amount < 0.0
    }

    pub fn abs_amount(&self) -> f64 {
        self.amount.abs()
    }
}
