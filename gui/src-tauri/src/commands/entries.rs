use tauri::State;

use money_core::models::{Entry, EntryKind};
use money_core::period::Period;
use money_core::services::entry_service::{self, EntryFilter};

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct ExpenseInput {
    pub date: String,
    pub amount: f64,
    pub from_account_id: i64,
    pub concept: String,
    pub subconcept: Option<String>,
    pub description: Option<String>,
}

#[tauri::command]
pub fn add_expense(state: State<AppState>, input: ExpenseInput) -> ApiResult<i64> {
    let conn = state.conn.lock().unwrap();
    Ok(entry_service::add_expense(
        &conn,
        &input.date,
        input.amount,
        input.from_account_id,
        &input.concept,
        input.subconcept.as_deref(),
        input.description.as_deref(),
    )?)
}

#[derive(serde::Deserialize)]
pub struct IncomeInput {
    pub date: String,
    pub amount: f64,
    pub to_account_id: i64,
    pub concept: String,
    pub description: Option<String>,
    pub split_emergency: bool,
}

#[derive(serde::Serialize)]
pub struct IncomeOutput {
    pub entry_id: i64,
    /// (fund account name, amount transferred), if the emergency split fired.
    pub emergency: Option<(String, f64)>,
}

#[tauri::command]
pub fn add_income(state: State<AppState>, input: IncomeInput) -> ApiResult<IncomeOutput> {
    let mut conn = state.conn.lock().unwrap();
    let result = entry_service::add_income_with_emergency_split(
        &mut conn,
        &input.date,
        input.amount,
        input.to_account_id,
        &input.concept,
        input.description.as_deref(),
        input.split_emergency,
    )?;
    Ok(IncomeOutput {
        entry_id: result.entry_id,
        emergency: result.emergency,
    })
}

#[derive(serde::Deserialize)]
pub struct TransferInput {
    pub date: String,
    pub amount: f64,
    pub from_account_id: i64,
    pub to_account_id: i64,
    pub description: Option<String>,
}

#[tauri::command]
pub fn add_transfer(state: State<AppState>, input: TransferInput) -> ApiResult<i64> {
    let conn = state.conn.lock().unwrap();
    Ok(entry_service::add_transfer(
        &conn,
        &input.date,
        input.amount,
        input.from_account_id,
        input.to_account_id,
        input.description.as_deref(),
    )?)
}

#[derive(serde::Deserialize, Default)]
pub struct EntryFilterInput {
    pub period: Option<String>,
    pub kind: Option<String>,
    pub concept: Option<String>,
    pub account_id: Option<i64>,
    pub limit: Option<u32>,
}

#[tauri::command]
pub fn list_entries(state: State<AppState>, filter: EntryFilterInput) -> ApiResult<Vec<Entry>> {
    let conn = state.conn.lock().unwrap();
    let period = match filter.period {
        Some(p) => Some(Period::parse(&p)?),
        None => None,
    };
    let kind = match filter.kind {
        Some(k) => Some(EntryKind::from_str(&k)?),
        None => None,
    };
    let f = EntryFilter {
        period,
        kind,
        concept: filter.concept,
        account_id: filter.account_id,
        limit: filter.limit,
    };
    Ok(entry_service::list(&conn, &f)?)
}

#[tauri::command]
pub fn delete_entry(state: State<AppState>, id: i64) -> ApiResult<()> {
    let conn = state.conn.lock().unwrap();
    entry_service::delete(&conn, id)?;
    Ok(())
}
