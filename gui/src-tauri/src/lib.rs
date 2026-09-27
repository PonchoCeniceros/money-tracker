mod commands;
mod error;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::accounts::list_accounts,
            commands::accounts::create_account,
            commands::accounts::archive_account,
            commands::accounts::reconcile_account,
            commands::entries::add_expense,
            commands::entries::add_income,
            commands::entries::add_transfer,
            commands::entries::list_entries,
            commands::entries::update_entry,
            commands::entries::delete_entry,
            commands::buckets::bucket_deposit,
            commands::buckets::bucket_withdraw,
            commands::report::monthly_report,
            commands::report::net_worth,
            commands::budgets::set_budget,
            commands::budgets::list_budgets,
            commands::budgets::delete_budget,
            commands::concepts::list_concepts,
            commands::concepts::add_concept,
            commands::config::get_config,
            commands::config::set_config,
            commands::config::list_config,
            commands::setup::is_seeded,
            commands::setup::seed,
            commands::backup::backup_create,
            commands::backup::backup_auto,
            commands::sync::ledger_status,
            commands::sync::connection_info,
            commands::sync::remote_login,
            commands::sync::remote_logout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
