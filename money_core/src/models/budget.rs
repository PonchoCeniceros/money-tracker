use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    pub id: Option<i64>,
    pub concept: String,
    pub monthly_limit: f64,
    pub month: i32,
    pub year: i32,
}
