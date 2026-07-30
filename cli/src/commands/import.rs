use clap::Args;
use money_core::db::open_db;
use money_core::services::import_service;
use money_core::Result;

#[derive(Args)]
pub struct ImportArgs {
    /// Path to the .ods file
    path: String,
}

pub fn run(args: ImportArgs) -> Result<()> {
    let conn = open_db()?;
    println!("Importing from {}...", args.path);
    let summary = import_service::import_ods(&conn, &args.path)?;
    println!(
        "✓ Imported {} transactions from {} sheets",
        summary.total_transactions,
        summary.sheets_processed.len()
    );
    if !summary.concepts.is_empty() {
        println!("  Concepts found: {}", summary.concepts.join(", "));
    }
    Ok(())
}
