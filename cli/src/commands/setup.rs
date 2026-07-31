use clap::Args;
use dialoguer::{Confirm, Input};
use money_core::db::open_db;
use money_core::period::Period;
use money_core::services::{account_service, setup_service};
use money_core::services::setup_service::SeedOptions;
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct SetupArgs {
    /// Repeatable: --account NAME=AMOUNT
    #[arg(long = "account", value_parser = parse_account_amount)]
    accounts: Vec<(String, f64)>,
    #[arg(short = 'D', long)]
    date: Option<String>,
    /// Allow seeding even if the database already has entries
    #[arg(long)]
    force: bool,
    #[arg(long)]
    yes: bool,
}

fn parse_account_amount(s: &str) -> std::result::Result<(String, f64), String> {
    let (name, amount) = s
        .split_once('=')
        .ok_or_else(|| "expected NAME=AMOUNT".to_string())?;
    let amount: f64 = amount.parse().map_err(|_| "invalid amount".to_string())?;
    Ok((name.to_string(), amount))
}

pub fn run(args: SetupArgs) -> Result<()> {
    let mut conn = open_db()?;

    if setup_service::is_seeded(&conn)? && !args.force {
        eprintln!("La base de datos ya tiene movimientos.");
        if args.yes {
            return Ok(());
        }
        let confirmed = helpers::map_dlg_err(
            Confirm::new()
                .with_prompt("¿Cargar saldos de todos modos?")
                .default(false)
                .interact(),
        )?;
        if !confirmed {
            println!("Cancelado.");
            return Ok(());
        }
    }

    let date = match &args.date {
        Some(d) => helpers::parse_date(Some(d))?,
        None => Period::current().start(),
    };

    let accounts = if !args.accounts.is_empty() {
        args.accounts
    } else {
        let existing = account_service::list_accounts(&conn, false)?;
        if existing.is_empty() {
            eprintln!("No hay cuentas. Crea al menos una con `money-tracker account add`.");
            return Ok(());
        }
        let mut collected = Vec::new();
        for account in existing {
            let amount: f64 = helpers::map_dlg_err(
                Input::new()
                    .with_prompt(format!("Saldo inicial de '{}' ($)", account.name))
                    .default(0.0)
                    .interact_text(),
            )?;
            if amount > 0.0 {
                collected.push((account.name, amount));
            }
        }
        collected
    };

    if accounts.is_empty() {
        println!("Nada que cargar.");
        return Ok(());
    }

    let summary = setup_service::seed(&mut conn, &SeedOptions { accounts, date: date.clone() })?;

    println!("✓ Saldos iniciales cargados ({date})");
    for (name, amount) in &summary.seeded {
        println!("    {:<25} ${amount:.2}", name);
    }
    Ok(())
}
