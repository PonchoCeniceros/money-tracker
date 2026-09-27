use tauri::State;



use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>, key: String) -> ApiResult<Option<String>> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(be.get_config(&key)?)
    })
    .await
}

#[tauri::command]
pub async fn set_config(state: State<'_, AppState>, key: String, value: String) -> ApiResult<()> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        be.set_config(&key, &value)?;
        Ok(())
    })
    .await
}

#[derive(serde::Serialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
}

#[tauri::command]
pub async fn list_config(state: State<'_, AppState>) -> ApiResult<Vec<ConfigEntry>> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(be
            .list_config()?
            .into_iter()
            .map(|c| ConfigEntry {
                key: c.key,
                value: c.value,
            })
            .collect())
    })
    .await
}