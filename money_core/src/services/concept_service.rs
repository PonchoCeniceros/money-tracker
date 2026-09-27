//! Concept vocabulary (`expense`, `income` or `both`).

use crate::error::Result;
use crate::models::Concept;
use crate::rules;
use crate::storage::LedgerBackend;

/// All concepts, or only those usable for `type_filter` (plus `both`).
pub fn list(be: &dyn LedgerBackend, type_filter: Option<&str>) -> Result<Vec<Concept>> {
    be.list_concepts(type_filter)
}

pub fn add(be: &dyn LedgerBackend, name: &str, concept_type: &str) -> Result<()> {
    rules::validate_concept_type(concept_type)?;
    be.add_concept(name, concept_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::MemoryBackend;

    #[test]
    fn unknown_type_is_rejected_before_writing() {
        let be = MemoryBackend::new();
        assert!(add(&be, "Mascotas", "gasto").is_err());
        assert!(list(&be, None).unwrap().is_empty());
        add(&be, "Mascotas", "expense").unwrap();
        assert_eq!(list(&be, Some("expense")).unwrap().len(), 1);
    }
}
