use tauri::State;

use money_core::models::Concept;


use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub fn list_concepts(state: State<AppState>, type_filter: Option<String>) -> ApiResult<Vec<Concept>> {
    let be = state.backend.lock().unwrap();
    Ok(be.list_concepts(type_filter.as_deref())?)
}

#[tauri::command]
pub fn add_concept(state: State<AppState>, name: String, concept_type: String) -> ApiResult<()> {
    if !["expense", "income", "both"].contains(&concept_type.as_str()) {
        return Err(money_core::AppError::Invalid("Type must be expense, income, or both".into()).into());
    }
    let be = state.backend.lock().unwrap();
    be.add_concept(&name, &concept_type)?;
    Ok(())
}