use clap::{Args, Subcommand};
use dialoguer::Confirm;
use money_core::db::open_db;
use money_core::models::{Entry, EntryKind};
use money_core::period::Period;
use money_core::services::entry_service::{self, EntryFilter, EntryUpdate};
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
    /// Correct a field on an existing entry (amount, concept, account, date, ...)
    Edit(EditArgs),
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
pub struct EditArgs {
    id: i64,
    #[arg(short = 'a', long)]
    amount: Option<f64>,
    #[arg(short = 'c', long)]
    concept: Option<String>,
    #[arg(short = 's', long)]
    subconcept: Option<String>,
    #[arg(short = 'd', long)]
    description: Option<String>,
    #[arg(short = 'D', long)]
    date: Option<String>,
    /// New source account (only valid on expenses and transfers)
    #[arg(long)]
    from: Option<String>,
    /// New destination account (only valid on incomes, transfers, and opening balances)
    #[arg(long)]
    to: Option<String>,
    #[arg(long)]
    yes: bool,
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
        EntryCommands::Edit(a) => edit(a),
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
        println!("{}", format_row(e));
    }
    Ok(())
}

fn format_row(e: &Entry) -> String {
    format!(
        "{:<6} {:<12} {:<10} {:>12} {:<15} {:<15} {:<20}",
        format!("#{}", e.id),
        e.date,
        e.kind.as_str(),
        format!("${:.2}", e.amount),
        e.from_account.as_deref().unwrap_or("—"),
        e.to_account.as_deref().unwrap_or("—"),
        e.concept.as_deref().unwrap_or("—"),
    )
}

fn edit(args: EditArgs) -> Result<()> {
    let conn = open_db()?;
    let current = entry_service::get(&conn, args.id)?;

    let date = match &args.date {
        Some(d) => Some(helpers::parse_date(Some(d))?),
        None => None,
    };
    let concept = match &args.concept {
        Some(c) => {
            let type_filter = match current.kind {
                EntryKind::Income => "income",
                _ => "expense",
            };
            Some(helpers::resolve_concept(&conn, c, type_filter)?)
        }
        None => None,
    };
    let from_account_id = match &args.from {
        Some(name) => Some(helpers::resolve_account(&conn, name)?.id),
        None => None,
    };
    let to_account_id = match &args.to {
        Some(name) => Some(helpers::resolve_account(&conn, name)?.id),
        None => None,
    };

    if date.is_none()
        && args.amount.is_none()
        && concept.is_none()
        && args.subconcept.is_none()
        && args.description.is_none()
        && from_account_id.is_none()
        && to_account_id.is_none()
    {
        println!("Nada que cambiar: {}", format_row(&current));
        return Ok(());
    }

    if !args.yes {
        let confirmed = helpers::map_dlg_err(
            Confirm::new()
                .with_prompt(format!(
                    "Antes: {}\n¿Aplicar los cambios a la entrada #{}?",
                    format_row(&current),
                    args.id
                ))
                .default(true)
                .interact(),
        )?;
        if !confirmed {
            println!("Cancelado.");
            return Ok(());
        }
    }

    let upd = EntryUpdate {
        date,
        amount: args.amount,
        concept,
        subconcept: args.subconcept,
        description: args.description,
        from_account_id,
        to_account_id,
    };
    let updated = entry_service::update(&conn, args.id, &upd)?;
    println!("✓ Entrada actualizada: {}", format_row(&updated));
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
