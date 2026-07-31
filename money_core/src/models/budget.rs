use serde::{Deserialize, Serialize};

/// `period` is stored as canonical "YYYY-MM" text, matching `entries` date
/// ranges rather than the old denormalized month/year integer pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct Budget {
    #[cfg_attr(feature = "ts-rs", ts(type = "number | null"))]
    pub id: Option<i64>,
    pub concept: String,
    pub monthly_limit: f64,
    pub period: String,
}
