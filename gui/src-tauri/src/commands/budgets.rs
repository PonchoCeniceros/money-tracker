//! Budgets are informative only; each command is one `budget_service` call.
use tauri::State;

use money_core::models::Budget;
use money_core::services::budget_service;


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
    let be = state.backend()?;
    budget_service::set(&**be, &input.concept, input.monthly_limit, &input.period)?;
    Ok(())
}

#[tauri::command]
pub fn list_budgets(state: State<AppState>, period: String) -> ApiResult<Vec<Budget>> {
    let be = state.backend()?;
    Ok(budget_service::list(&**be, Some(&period))?)
}

#[tauri::command]
pub fn delete_budget(state: State<AppState>, concept: String, period: String) -> ApiResult<()> {
    let be = state.backend()?;
    budget_service::remove(&**be, &concept, &period)?;
    Ok(())
}