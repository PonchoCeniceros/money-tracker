use tauri::State;

use money_core::period::Period;
use money_core::services::report_service::{self, MonthlyReport, NetWorth};

use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub fn monthly_report(state: State<AppState>, period: String) -> ApiResult<MonthlyReport> {
    let conn = state.conn.lock().unwrap();
    let p = Period::parse(&period)?;
    Ok(report_service::monthly_report(&conn, &p)?)
}

#[tauri::command]
pub fn net_worth(state: State<AppState>, as_of: Option<String>) -> ApiResult<NetWorth> {
    let conn = state.conn.lock().unwrap();
    Ok(report_service::net_worth(&conn, as_of.as_deref())?)
}
