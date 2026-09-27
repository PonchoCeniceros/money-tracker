use tauri::State;

use money_core::services::setup_service::{self, SeedOptions};

use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub async fn is_seeded(state: State<'_, AppState>) -> ApiResult<bool> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(setup_service::is_seeded(&**be)?)
    })
    .await
}

#[derive(serde::Deserialize)]
pub struct SeedInput {
    pub accounts: Vec<(String, f64)>,
    pub date: String,
}

#[derive(serde::Serialize)]
pub struct SeedOutput {
    pub seeded: Vec<(String, f64)>,
}

#[tauri::command]
pub async fn seed(state: State<'_, AppState>, input: SeedInput) -> ApiResult<SeedOutput> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        let summary = setup_service::seed(
            &**be,
            &SeedOptions {
                accounts: input.accounts,
                date: input.date,
            },
        )?;
        Ok(SeedOutput {
            seeded: summary.seeded,
        })
    })
    .await
}