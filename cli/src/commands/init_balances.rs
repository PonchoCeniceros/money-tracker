use clap::Args;
use dialoguer::{Confirm, Input};
use money_core::db::open_db;
use money_core::services::{bucket_service, init_service};
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct InitBalancesArgs {
    #[arg(short = 'f', long)]
    flujo: Option<f64>,
}

pub fn run(args: InitBalancesArgs) -> Result<()> {
    let conn = open_db()?;
    let month = helpers::get_current_month();
    let year = helpers::get_current_year();
    let date = helpers::get_today();

    let has_txns: bool = conn
        .query_row("SELECT COUNT(*) > 0 FROM transactions", [], |row| row.get(0))
        .unwrap_or(false);
    if has_txns {
        println!("Warning: Database already has transactions.");
        if !helpers::map_dlg_err(
            Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt("Continue anyway?")
                .default(false)
                .interact(),
        )? {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let flujo = match args.flujo {
        Some(v) => {
            if v <= 0.0 {
                eprintln!("Flujo inicial must be positive");
                return Ok(());
            }
            v
        }
        None => helpers::map_dlg_err(
            Input::new()
                .with_prompt("Initial cash balance (flujo inicial) ($)")
                .interact_text(),
        )?,
    };

    init_service::init_flujo(&conn, flujo, &date, month, year)?;
    println!("✓ Initial flujo set: ${:.2}", flujo);

    let buckets = bucket_service::list_buckets(&conn)?;
    if !buckets.is_empty() {
        println!();
        println!("Now set initial balances for your buckets:");
        for b in &buckets {
            let current = b.current_balance;
            if current > 0.0 {
                println!(
                    "  '{}' already has ${:.2}. Set new balance?",
                    b.name, current
                );
                let new_balance: f64 = helpers::map_dlg_err(
                    Input::new()
                        .with_prompt(format!("  New balance for '{}' ($)", b.name))
                        .default(0.0)
                        .interact_text(),
                )?;
                if new_balance < 0.0 {
                    eprintln!("  Balance must be positive, skipping '{}'", b.name);
                    continue;
                }
                let diff = new_balance - current;
                if diff > 0.0 {
                    init_service::init_bucket_balance(
                        &conn,
                        b.id.unwrap(),
                        diff,
                        &date,
                        month,
                        year,
                    )?;
                    println!("  ✓ ${:.2} added to '{}'", diff, b.name);
                } else if diff < 0.0 {
                    eprintln!(
                        "  Cannot reduce balance automatically. Withdraw manually with `money-tracker bucket withdraw`"
                    );
                } else {
                    println!("  ✓ Balance unchanged for '{}'", b.name);
                }
            } else {
                let bal: f64 = helpers::map_dlg_err(
                    Input::new()
                        .with_prompt(format!("  Initial balance for '{}' ($)", b.name))
                        .default(0.0)
                        .interact_text(),
                )?;
                if bal > 0.0 {
                    init_service::init_bucket_balance(
                        &conn,
                        b.id.unwrap(),
                        bal,
                        &date,
                        month,
                        year,
                    )?;
                    println!("  ✓ ${:.2} deposited to '{}'", bal, b.name);
                }
            }
        }
    } else {
        println!("No buckets yet. Create some with `money-tracker bucket create`");
    }

    println!();
    println!("Done. Run `money-tracker report` to verify.");
    Ok(())
}
