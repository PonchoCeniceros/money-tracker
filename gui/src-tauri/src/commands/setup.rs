use tauri::State;

use money_core::services::setup_service::{self, SeedOptions};

use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub fn is_seeded(state: State<AppState>) -> ApiResult<bool> {
    let conn = state.conn.lock().unwrap();
    Ok(setup_service::is_seeded(&conn)?)
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
pub fn seed(state: State<AppState>, input: SeedInput) -> ApiResult<SeedOutput> {
    let mut conn = state.conn.lock().unwrap();
    let summary = setup_service::seed(
        &mut conn,
        &SeedOptions {
            accounts: input.accounts,
            date: input.date,
        },
    )?;
    Ok(SeedOutput {
        seeded: summary.seeded,
    })
}
