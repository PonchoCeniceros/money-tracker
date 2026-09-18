//! Budgets are informative-only rows with no invariants beyond the schema's
//! own CHECK/UNIQUE constraints; these thin commands go straight to the
//! backend's `set_budget`/`list_budgets`/`delete_budget`.
use tauri::State;

use money_core::models::Budget;


use crate::error::ApiResult;
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct SetBudgetInput {
    pub concept: String,
    pub monthly_limit: f64,
    pub period: String,
}

#[tauri::command]
pub fn set_budget(state: State<AppState>, input: SetBudgetInput) -> ApiResult<()> {
    let be = state.backend.lock().unwrap();
    be.set_budget(&input.concept, input.monthly_limit, &input.period)?;
    Ok(())
}

#[tauri::command]
pub fn list_budgets(state: State<AppState>, period: String) -> ApiResult<Vec<Budget>> {
    let be = state.backend.lock().unwrap();
    Ok(be.list_budgets(Some(&period))?)
}

#[tauri::command]
pub fn delete_budget(state: State<AppState>, concept: String, period: String) -> ApiResult<()> {
    let be = state.backend.lock().unwrap();
    be.delete_budget(&concept, &period)?;
    Ok(())
}