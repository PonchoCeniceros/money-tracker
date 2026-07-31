use clap::{Args, Subcommand};
use dialoguer::Confirm;
use money_core::db::open_db;
use money_core::models::EntryKind;
use money_core::period::Period;
use money_core::services::entry_service::{self, EntryFilter};
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct EntryArgs {
    #[command(subcommand)]
    command: EntryCommands,
}

#[derive(Subcommand)]
enum EntryCommands {
    /// List entries, optionally filtered
    List(ListArgs),
    /// Delete an entry by id
    Rm(RmArgs),
}

#[derive(Args)]
pub struct ListArgs {
    #[arg(short = 'p', long)]
    period: Option<String>,
    #[arg(short = 'c', long)]
    concept: Option<String>,
    #[arg(long)]
    account: Option<String>,
    #[arg(long)]
    kind: Option<String>,
    #[arg(short = 'n', long)]
    limit: Option<u32>,
}

#[derive(Args)]
pub struct RmArgs {
    id: i64,
    #[arg(long)]
    yes: bool,
}

pub fn run(args: EntryArgs) -> Result<()> {
    match args.command {
        EntryCommands::List(a) => list(a),
        EntryCommands::Rm(a) => rm(a),
    }
}

fn list(args: ListArgs) -> Result<()> {
    let conn = open_db()?;

    let period = match args.period {
        Some(p) => Some(Period::parse(&p)?),
        None => None,
    };
    let kind = match args.kind {
        Some(k) => Some(EntryKind::from_str(&k)?),
        None => None,
    };
    let account_id = match args.account {
        Some(name) => Some(helpers::resolve_account(&conn, &name)?.id),
        None => None,
    };

    let filter = EntryFilter {
        period,
        kind,
        concept: args.concept,
        account_id,
        limit: args.limit,
    };

    let entries = entry_service::list(&conn, &filter)?;
    if entries.is_empty() {
        println!("No hay movimientos.");
        return Ok(());
    }

    println!(
        "{:<6} {:<12} {:<10} {:>12} {:<15} {:<15} {:<20}",
        "#", "FECHA", "TIPO", "MONTO", "DE", "A", "CONCEPTO"
    );
    println!("{}", "-".repeat(95));
    for e in &entries {
        println!(
            "{:<6} {:<12} {:<10} {:>12} {:<15} {:<15} {:<20}",
            format!("#{}", e.id),
            e.date,
            e.kind.as_str(),
            format!("${:.2}", e.amount),
            e.from_account.as_deref().unwrap_or("—"),
            e.to_account.as_deref().unwrap_or("—"),
            e.concept.as_deref().unwrap_or("—"),
        );
    }
    Ok(())
}

fn rm(args: RmArgs) -> Result<()> {
    let conn = open_db()?;
    if !args.yes {
        let confirmed = helpers::map_dlg_err(
            Confirm::new()
                .with_prompt(format!("¿Borrar la entrada #{}?", args.id))
                .default(false)
                .interact(),
        )?;
        if !confirmed {
            println!("Cancelado.");
            return Ok(());
        }
    }
    entry_service::delete(&conn, args.id)?;
    println!("✓ Entrada #{} borrada", args.id);
    Ok(())
}
