use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub id: Option<i64>,
    pub name: String,
    pub concept_type: String,
}
