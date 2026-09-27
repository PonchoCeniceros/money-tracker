//! Sugar over `entry_service::add_transfer` for the two bucket operations,
//! matching `cli/src/commands/bucket.rs`. Deposit/withdraw are transfers,
//! never expenses.
use tauri::State;

use money_core::services::entry_service;

use crate::error::ApiResult;
use crate::state::AppState;

#[tauri::command]
pub async fn bucket_deposit(
    state: State<'_, AppState>,
    bucket_id: i64,
    from_account_id: i64,
    amount: f64,
    date: String,
) -> ApiResult<i64> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(entry_service::add_transfer(
            &**be,
            &date,
            amount,
            from_account_id,
            bucket_id,
            None,
        )?)
    })
    .await
}

/// NOT an expense — moves money from the bucket into a spending account.
/// If the caller already spent it, they still need `add_expense`.
#[tauri::command]
pub async fn bucket_withdraw(
    state: State<'_, AppState>,
    bucket_id: i64,
    to_account_id: i64,
    amount: f64,
    date: String,
) -> ApiResult<i64> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(entry_service::add_transfer(
            &**be,
            &date,
            amount,
            bucket_id,
            to_account_id,
            None,
        )?)
    })
    .await
}
