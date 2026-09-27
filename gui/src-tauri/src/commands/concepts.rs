use tauri::State;

use money_core::models::Concept;
use money_core::services::concept_service;


use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub fn list_concepts(state: State<AppState>, type_filter: Option<String>) -> ApiResult<Vec<Concept>> {
    let be = state.backend()?;
    Ok(concept_service::list(&**be, type_filter.as_deref())?)
}

#[tauri::command]
pub fn add_concept(state: State<AppState>, name: String, concept_type: String) -> ApiResult<()> {
    let be = state.backend()?;
    concept_service::add(&**be, &name, &concept_type)?;
    Ok(())
}