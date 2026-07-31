//! Bucket deposit/withdraw — kept under this name because it's the user's
//! own vocabulary for operations (c) and (d). Creating and listing buckets
//! now lives in `account` (buckets are just accounts of kind
//! emergency/target); this module is left with only the two movements.
use clap::{Args, Subcommand};
use dialoguer::{FuzzySelect, Input};
use money_core::db::open_db;
use money_core::services::{account_service, entry_service};
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct BucketArgs {
    #[command(subcommand)]
    command: BucketCommands,
}

#[derive(Subcommand)]
enum BucketCommands {
    /// Deposit into a savings bucket (transfer, not an expense)
    Deposit(DepositArgs),
    /// Withdraw from a savings bucket into a spending account.
    /// This is a transfer, NOT an expense — if you already spent it,
    /// also register the expense with `add ... --from <account>`.
    Withdraw(WithdrawArgs),
}

#[derive(Args)]
pub struct DepositArgs {
    #[arg(short = 'b', long)]
    bucket: Option<String>,
    #[arg(short = 'a', long)]
    amount: Option<f64>,
    #[arg(short = 'f', long)]
    from: Option<String>,
    #[arg(short = 'D', long)]
    date: Option<String>,
}

#[derive(Args)]
pub struct WithdrawArgs {
    #[arg(short = 'b', long)]
    bucket: Option<String>,
    #[arg(short = 'a', long)]
    amount: Option<f64>,
    #[arg(short = 't', long)]
    to: Option<String>,
    #[arg(short = 'D', long)]
    date: Option<String>,
}

pub fn run(args: BucketArgs) -> Result<()> {
    match args.command {
        BucketCommands::Deposit(a) => deposit(a),
        BucketCommands::Withdraw(a) => withdraw(a),
    }
}

fn pick_bucket_name(conn: &rusqlite::Connection, given: Option<String>) -> Result<String> {
    match given {
        Some(b) => Ok(b),
        None => {
            let buckets: Vec<String> = account_service::list_accounts(conn, false)?
                .into_iter()
                .filter(|a| {
                    matches!(
                        a.kind,
                        money_core::models::AccountKind::Emergency
                            | money_core::models::AccountKind::Target
                    )
                })
                .map(|a| a.name)
                .collect();
            let selection = helpers::map_dlg_err(
                FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("Bucket")
                    .items(&buckets)
                    .default(0)
                    .interact(),
            )?;
            Ok(buckets[selection].clone())
        }
    }
}

fn prompt_amount() -> Result<f64> {
    helpers::map_dlg_err(
        Input::new()
            .with_prompt("Amount ($)")
            .validate_with(|v: &f64| if *v > 0.0 { Ok(()) } else { Err("Amount must be positive") })
            .interact_text(),
    )
}

fn deposit(args: DepositArgs) -> Result<()> {
    let conn = open_db()?;
    let bucket_name = pick_bucket_name(&conn, args.bucket)?;
    let bucket = helpers::resolve_account(&conn, &bucket_name)?;

    let amount = match args.amount {
        Some(a) if a > 0.0 => a,
        Some(_) => {
            eprintln!("Amount must be positive");
            return Ok(());
        }
        None => prompt_amount()?,
    };

    let from = match args.from {
        Some(f) => helpers::resolve_account(&conn, &f)?,
        None => account_service::default_account(&conn)?,
    };

    let date = helpers::parse_date(args.date.as_deref())?;

    entry_service::add_transfer(&conn, &date, amount, from.id, bucket.id, None)?;
    let new_balance = account_service::get_account(&conn, bucket.id)?;

    println!(
        "✓ ${amount:.2} depositado a '{}' (saldo: ${:.2})",
        bucket.name, new_balance.balance
    );
    if let Some(pct) = new_balance.progress_pct() {
        println!(
            "  {}: ${:.2} / ${:.2} ({pct:.0}%)",
            bucket.name,
            new_balance.balance,
            new_balance.target_amount.unwrap_or(0.0)
        );
    }
    Ok(())
}

fn withdraw(args: WithdrawArgs) -> Result<()> {
    let conn = open_db()?;
    let bucket_name = pick_bucket_name(&conn, args.bucket)?;
    let bucket = helpers::resolve_account(&conn, &bucket_name)?;

    let amount = match args.amount {
        Some(a) if a > 0.0 => a,
        Some(_) => {
            eprintln!("Amount must be positive");
            return Ok(());
        }
        None => prompt_amount()?,
    };

    let to = match args.to {
        Some(t) => helpers::resolve_account(&conn, &t)?,
        None => account_service::default_account(&conn)?,
    };

    let date = helpers::parse_date(args.date.as_deref())?;

    entry_service::add_transfer(&conn, &date, amount, bucket.id, to.id, None)?;

    println!("✓ ${amount:.2} movido de '{}' a '{}'", bucket.name, to.name);
    println!(
        "  Esto NO es un gasto. Si ya lo gastaste: money-tracker add {amount} <concepto> --from {}",
        to.name
    );
    Ok(())
}
