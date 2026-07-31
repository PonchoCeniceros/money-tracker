use clap::{Args, Subcommand};
use dialoguer::Confirm;
use money_core::db as core_db;
use money_core::services::account_service;
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct DbArgs {
    #[command(subcommand)]
    command: DbCommands,
}

#[derive(Subcommand)]
enum DbCommands {
    /// Show the database path, schema version, and record counts
    Status,
    /// Move the current database aside so a fresh one can be created
    Reset(ResetArgs),
}

#[derive(Args)]
pub struct ResetArgs {
    /// Keep the old file instead of discarding it (recommended)
    #[arg(long, default_value_t = true)]
    backup: bool,
    #[arg(long)]
    yes: bool,
}

pub fn run(args: DbArgs) -> Result<()> {
    match args.command {
        DbCommands::Status => status(),
        DbCommands::Reset(a) => reset(a),
    }
}

fn status() -> Result<()> {
    let path = core_db::db_path();
    println!("Ruta: {}", path.display());

    if !path.exists() {
        println!("(no existe todavía — se crea al primer comando)");
        return Ok(());
    }

    let conn = core_db::open_db()?;
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let entries: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
    let accounts = account_service::list_accounts(&conn, true)?;

    println!("Esquema: v{version}");
    println!("Cuentas: {} ({} archivadas)", accounts.len(), accounts.iter().filter(|a| a.archived).count());
    println!("Movimientos: {entries}");
    Ok(())
}

fn reset(args: ResetArgs) -> Result<()> {
    let path = core_db::db_path();
    if !path.exists() {
        println!("No hay base de datos en {}", path.display());
        return Ok(());
    }

    if !args.yes {
        let confirmed = helpers::map_dlg_err(
            Confirm::new()
                .with_prompt(format!(
                    "¿Mover {} a un lado y empezar limpio?",
                    path.display()
                ))
                .default(false)
                .interact(),
        )?;
        if !confirmed {
            println!("Cancelado.");
            return Ok(());
        }
    }

    if args.backup {
        let timestamp = chrono::Local::now().format("%Y%m%d%H%M%S");
        let mut backup_path = path.clone();
        backup_path.set_file_name(format!(
            "{}.backup-{timestamp}",
            path.file_name().unwrap().to_string_lossy()
        ));
        std::fs::rename(&path, &backup_path)?;
        println!("✓ Respaldado en {}", backup_path.display());
    } else {
        std::fs::remove_file(&path)?;
        println!("✓ Base de datos eliminada");
    }
    Ok(())
}
