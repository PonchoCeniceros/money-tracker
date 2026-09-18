use tauri::State;



use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub fn get_config(state: State<AppState>, key: String) -> ApiResult<Option<String>> {
    let be = state.backend.lock().unwrap();
    Ok(be.get_config(&key)?)
}

#[tauri::command]
pub fn set_config(state: State<AppState>, key: String, value: String) -> ApiResult<()> {
    let be = state.backend.lock().unwrap();
    be.set_config(&key, &value)?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
}

#[tauri::command]
pub fn list_config(state: State<AppState>) -> ApiResult<Vec<ConfigEntry>> {
    let be = state.backend.lock().unwrap();
    Ok(be
        .list_config()?
        .into_iter()
        .map(|c| ConfigEntry {
            key: c.key,
            value: c.value,
        })
        .collect())
}