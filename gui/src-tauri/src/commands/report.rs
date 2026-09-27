use tauri::State;

use money_core::period::Period;
use money_core::services::report_service::{self, MonthlyReport, NetWorth};

use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub async fn monthly_report(state: State<'_, AppState>, period: String) -> ApiResult<MonthlyReport> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        let p = Period::parse(&period)?;
        Ok(report_service::monthly_report(&**be, &p)?)
    })
    .await
}

#[tauri::command]
pub async fn net_worth(state: State<'_, AppState>, as_of: Option<String>) -> ApiResult<NetWorth> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(report_service::net_worth(&**be, as_of.as_deref())?)
    })
    .await
}
