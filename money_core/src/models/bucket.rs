use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bucket {
    pub id: Option<i64>,
    pub name: String,
    pub bucket_type: String,
    pub target_amount: Option<f64>,
    pub savings_percentage: Option<f64>,
    pub current_balance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketMovement {
    pub id: Option<i64>,
    pub bucket_id: i64,
    pub date: String,
    pub amount: f64,
    pub description: Option<String>,
    pub month: i32,
    pub year: i32,
}

impl Bucket {
    pub fn progress_pct(&self) -> Option<f64> {
        self.target_amount.map(|t| {
            if t > 0.0 {
                (self.current_balance / t * 100.0).min(100.0)
            } else {
                0.0
            }
        })
    }
}
