use tauri::State;

use money_core::models::{Entry, EntryKind};
use money_core::period::Period;
use money_core::services::entry_service::{self, EntryFilter, EntryUpdate};

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
pub async fn add_expense(state: State<'_, AppState>, input: ExpenseInput) -> ApiResult<i64> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(entry_service::add_expense(
            &**be,
            &input.date,
            input.amount,
            input.from_account_id,
            &input.concept,
            input.subconcept.as_deref(),
            input.description.as_deref(),
        )?)
    })
    .await
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
pub async fn add_income(state: State<'_, AppState>, input: IncomeInput) -> ApiResult<IncomeOutput> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        let result = entry_service::add_income_with_emergency_split(
            &**be,
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
    })
    .await
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
pub async fn add_transfer(state: State<'_, AppState>, input: TransferInput) -> ApiResult<i64> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(entry_service::add_transfer(
            &**be,
            &input.date,
            input.amount,
            input.from_account_id,
            input.to_account_id,
            input.description.as_deref(),
        )?)
    })
    .await
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
pub async fn list_entries(state: State<'_, AppState>, filter: EntryFilterInput) -> ApiResult<Vec<Entry>> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
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
            ..Default::default()
        };
        Ok(entry_service::list(&**be, &f)?)
    })
    .await
}

#[derive(serde::Deserialize, Default)]
pub struct EntryUpdateInput {
    pub date: Option<String>,
    pub amount: Option<f64>,
    pub concept: Option<String>,
    pub subconcept: Option<String>,
    pub description: Option<String>,
    pub from_account_id: Option<i64>,
    pub to_account_id: Option<i64>,
}

#[tauri::command]
pub async fn update_entry(state: State<'_, AppState>, id: i64, input: EntryUpdateInput) -> ApiResult<Entry> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        let upd = EntryUpdate {
            date: input.date,
            amount: input.amount,
            concept: input.concept,
            subconcept: input.subconcept,
            description: input.description,
            from_account_id: input.from_account_id,
            to_account_id: input.to_account_id,
        };
        Ok(entry_service::update(&**be, id, &upd)?)
    })
    .await
}

#[tauri::command]
pub async fn delete_entry(state: State<'_, AppState>, id: i64) -> ApiResult<()> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        entry_service::delete(&**be, id)?;
        Ok(())
    })
    .await
}

/// What an income of `amount` into `to_account_id` would put in the emergency
/// fund (`null` = no split: restricted account, no fund, or nothing to split).
#[tauri::command]
pub async fn income_split_preview(
    state: State<'_, AppState>,
    to_account_id: i64,
    amount: f64,
) -> ApiResult<Option<entry_service::SplitPreview>> {
    let be = state.backend().await?;
    crate::state::blocking(move || {
        Ok(entry_service::emergency_split_preview(&**be, to_account_id, amount)?)
    })
    .await
}
