use rusqlite::OptionalExtension;
use tauri::State;

use money_core::AppError;

use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub fn get_config(state: State<AppState>, key: String) -> ApiResult<Option<String>> {
    let conn = state.conn.lock().unwrap();
    let value = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::from)?;
    Ok(value)
}

#[tauri::command]
pub fn set_config(state: State<AppState>, key: String, value: String) -> ApiResult<()> {
    let conn = state.conn.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )
    .map_err(AppError::from)?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
}

#[tauri::command]
pub fn list_config(state: State<AppState>) -> ApiResult<Vec<ConfigEntry>> {
    let conn = state.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT key, value FROM config ORDER BY key")
        .map_err(AppError::from)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ConfigEntry {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })
        .map_err(AppError::from)?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(AppError::from)?);
    }
    Ok(result)
}
