//! `money_core` has no dedicated budget service (mirroring
//! `cli/src/commands/budget.rs`, which also runs plain SQL against
//! `budgets` directly) — budgets are informative-only rows with no
//! invariants beyond the schema's own CHECK/UNIQUE constraints.
use tauri::State;

use money_core::models::Budget;
use money_core::AppError;

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
    let conn = state.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO budgets (concept, monthly_limit, period) VALUES (?1, ?2, ?3)
         ON CONFLICT(concept, period) DO UPDATE SET monthly_limit = excluded.monthly_limit",
        rusqlite::params![input.concept, input.monthly_limit, input.period],
    )
    .map_err(AppError::from)?;
    Ok(())
}

#[tauri::command]
pub fn list_budgets(state: State<AppState>, period: String) -> ApiResult<Vec<Budget>> {
    let conn = state.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, concept, monthly_limit, period FROM budgets WHERE period = ?1 ORDER BY concept")
        .map_err(AppError::from)?;
    let rows = stmt
        .query_map(rusqlite::params![period], |row| {
            Ok(Budget {
                id: row.get(0)?,
                concept: row.get(1)?,
                monthly_limit: row.get(2)?,
                period: row.get(3)?,
            })
        })
        .map_err(AppError::from)?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(AppError::from)?);
    }
    Ok(result)
}

#[tauri::command]
pub fn delete_budget(state: State<AppState>, concept: String, period: String) -> ApiResult<()> {
    let conn = state.conn.lock().unwrap();
    conn.execute(
        "DELETE FROM budgets WHERE concept = ?1 AND period = ?2",
        rusqlite::params![concept, period],
    )
    .map_err(AppError::from)?;
    Ok(())
}
