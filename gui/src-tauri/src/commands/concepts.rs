use tauri::State;

use money_core::models::Concept;
use money_core::services::concept_service;


use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub async fn list_concepts(state: State<'_, AppState>, type_filter: Option<String>) -> ApiResult<Vec<Concept>> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(concept_service::list(&**be, type_filter.as_deref())?)
    })
    .await
}

#[tauri::command]
pub async fn add_concept(state: State<'_, AppState>, name: String, concept_type: String) -> ApiResult<()> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        concept_service::add(&**be, &name, &concept_type)?;
        Ok(())
    })
    .await
}