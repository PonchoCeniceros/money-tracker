use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct Concept {
    #[cfg_attr(feature = "ts-rs", ts(type = "number | null"))]
    pub id: Option<i64>,
    pub name: String,
    pub concept_type: String,
}
