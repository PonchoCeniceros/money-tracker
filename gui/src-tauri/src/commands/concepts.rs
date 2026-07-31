use tauri::State;

use money_core::models::Concept;
use money_core::AppError;

use crate::error::ApiResult;
use crate::state::AppState;

fn row_to_concept(row: &rusqlite::Row) -> rusqlite::Result<Concept> {
    Ok(Concept {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        concept_type: row.get(2)?,
    })
}

#[tauri::command]
pub fn list_concepts(state: State<AppState>, type_filter: Option<String>) -> ApiResult<Vec<Concept>> {
    let conn = state.conn.lock().unwrap();
    let mut result = Vec::new();

    match type_filter {
        Some(t) => {
            let mut stmt = conn
                .prepare("SELECT id, name, concept_type FROM concepts WHERE concept_type IN (?1, 'both') ORDER BY name")
                .map_err(AppError::from)?;
            let rows = stmt.query_map(rusqlite::params![t], row_to_concept).map_err(AppError::from)?;
            for row in rows {
                result.push(row.map_err(AppError::from)?);
            }
        }
        None => {
            let mut stmt = conn
                .prepare("SELECT id, name, concept_type FROM concepts ORDER BY concept_type, name")
                .map_err(AppError::from)?;
            let rows = stmt.query_map([], row_to_concept).map_err(AppError::from)?;
            for row in rows {
                result.push(row.map_err(AppError::from)?);
            }
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn add_concept(state: State<AppState>, name: String, concept_type: String) -> ApiResult<()> {
    if !["expense", "income", "both"].contains(&concept_type.as_str()) {
        return Err(AppError::Invalid("Type must be expense, income, or both".into()).into());
    }
    let conn = state.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO concepts (name, concept_type) VALUES (?1, ?2)",
        rusqlite::params![name, concept_type],
    )
    .map_err(AppError::from)?;
    Ok(())
}
