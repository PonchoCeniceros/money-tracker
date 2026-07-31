use tauri::State;

use money_core::models::{AccountBalance, AccountKind, NewAccount};
use money_core::services::account_service;

use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub fn list_accounts(state: State<AppState>, include_archived: bool) -> ApiResult<Vec<AccountBalance>> {
    let conn = state.conn.lock().unwrap();
    Ok(account_service::list_accounts(&conn, include_archived)?)
}

#[derive(serde::Deserialize)]
pub struct NewAccountInput {
    pub name: String,
    pub kind: String,
    pub target_amount: Option<f64>,
    pub credit_limit: Option<f64>,
    pub restricted: bool,
}

#[tauri::command]
pub fn create_account(state: State<AppState>, input: NewAccountInput) -> ApiResult<i64> {
    let conn = state.conn.lock().unwrap();
    let kind = AccountKind::from_str(&input.kind)?;
    let new_account = match kind {
        AccountKind::Spending => NewAccount::spending(&input.name),
        AccountKind::Emergency => NewAccount::emergency(&input.name),
        AccountKind::Target => NewAccount::target(&input.name, input.target_amount)?,
        AccountKind::Credit => NewAccount::credit(&input.name, input.credit_limit),
    };
    let new_account = if input.restricted {
        new_account.restricted()
    } else {
        new_account
    };
    Ok(account_service::create_account(&conn, &new_account)?)
}

#[tauri::command]
pub fn archive_account(state: State<AppState>, id: i64, force: bool) -> ApiResult<()> {
    let conn = state.conn.lock().unwrap();
    account_service::archive_account(&conn, id, force)?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct ReconcileOutput {
    pub entry_id: Option<i64>,
    pub diff: f64,
}

#[tauri::command]
pub fn reconcile_account(
    state: State<AppState>,
    id: i64,
    actual: f64,
    concept: String,
    date: String,
) -> ApiResult<ReconcileOutput> {
    let conn = state.conn.lock().unwrap();
    let result = account_service::reconcile_account(&conn, id, actual, &concept, &date)?;
    Ok(ReconcileOutput {
        entry_id: result.entry_id,
        diff: result.diff,
    })
}
