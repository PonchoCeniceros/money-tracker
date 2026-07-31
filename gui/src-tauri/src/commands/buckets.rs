//! Sugar over `entry_service::add_transfer` for the two bucket operations,
//! matching `cli/src/commands/bucket.rs`. Deposit/withdraw are transfers,
//! never expenses.
use tauri::State;

use money_core::services::entry_service;

use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub fn bucket_deposit(
    state: State<AppState>,
    bucket_id: i64,
    from_account_id: i64,
    amount: f64,
    date: String,
) -> ApiResult<i64> {
    let conn = state.conn.lock().unwrap();
    Ok(entry_service::add_transfer(
        &conn,
        &date,
        amount,
        from_account_id,
        bucket_id,
        None,
    )?)
}

/// NOT an expense — moves money from the bucket into a spending account.
/// If the caller already spent it, they still need `add_expense`.
#[tauri::command]
pub fn bucket_withdraw(
    state: State<AppState>,
    bucket_id: i64,
    to_account_id: i64,
    amount: f64,
    date: String,
) -> ApiResult<i64> {
    let conn = state.conn.lock().unwrap();
    Ok(entry_service::add_transfer(
        &conn,
        &date,
        amount,
        bucket_id,
        to_account_id,
        None,
    )?)
}
