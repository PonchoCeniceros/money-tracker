//! Backups from the GUI: the "Respaldar ahora" button and the lazy automatic
//! backup the frontend asks for once the app has connected.
use tauri::State;

use money_core::models::BackupInfo;
use money_core::services::backup_service;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct BackupInput {
    /// Folder or new file; empty/absent = ~/.money-tracker/backups/.
    pub dest: Option<String>,
}

#[tauri::command]
pub async fn backup_create(state: State<'_, AppState>, input: BackupInput) -> ApiResult<BackupInfo> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        let dest = input.dest.filter(|d| !d.trim().is_empty()).map(std::path::PathBuf::from);
        Ok(backup_service::create(&**be, dest.as_deref())?)
    })
    .await
}

/// `null` when no backup was due. An error means it was due but failed; the
/// frontend shows it as a warning and the next launch retries.
#[tauri::command]
pub async fn backup_auto(state: State<'_, AppState>) -> ApiResult<Option<BackupInfo>> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(backup_service::run_auto_with(&**be).transpose()?)
    })
    .await
}
