//! Monthly budgets: informative only, never block a write.

use crate::error::Result;
use crate::models::Budget;
use crate::rules;
use crate::storage::LedgerBackend;

/// Creates or replaces the budget for `concept` in `period` (`YYYY-MM`).
pub fn set(be: &dyn LedgerBackend, concept: &str, limit: f64, period: &str) -> Result<()> {
    rules::validate_budget_limit(limit)?;
    be.set_budget(concept, limit, period)
}

pub fn list(be: &dyn LedgerBackend, period: Option<&str>) -> Result<Vec<Budget>> {
    be.list_budgets(period)
}

pub fn remove(be: &dyn LedgerBackend, concept: &str, period: &str) -> Result<()> {
    be.delete_budget(concept, period)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::MemoryBackend;

    #[test]
    fn non_positive_limit_is_rejected_before_writing() {
        let be = MemoryBackend::seeded();
        assert!(set(&be, "Alimentos", 0.0, "2026-08").is_err());
        assert!(list(&be, None).unwrap().is_empty());
        set(&be, "Alimentos", 2500.0, "2026-08").unwrap();
        assert_eq!(list(&be, Some("2026-08")).unwrap().len(), 1);
    }
}
