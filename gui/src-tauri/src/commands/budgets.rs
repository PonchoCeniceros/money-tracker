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
pub async fn set_budget(state: State<'_, AppState>, input: SetBudgetInput) -> ApiResult<()> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        budget_service::set(&**be, &input.concept, input.monthly_limit, &input.period)?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn list_budgets(state: State<'_, AppState>, period: String) -> ApiResult<Vec<Budget>> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(budget_service::list(&**be, Some(&period))?)
    })
    .await
}

#[tauri::command]
pub async fn delete_budget(state: State<'_, AppState>, concept: String, period: String) -> ApiResult<()> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        budget_service::remove(&**be, &concept, &period)?;
        Ok(())
    })
    .await
}