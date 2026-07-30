use clap::{Args, Subcommand};
use dialoguer::{FuzzySelect, Input};
use money_core::db::open_db;
use money_core::models::Bucket;
use money_core::services::bucket_service;
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct BucketArgs {
    #[command(subcommand)]
    command: BucketCommands,
}

#[derive(Subcommand)]
enum BucketCommands {
    Create(CreateArgs),
    List,
    Deposit(DepositArgs),
    Withdraw(WithdrawArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    name: Option<String>,
    #[arg(short = 't', long)]
    target: Option<f64>,
}

#[derive(Args)]
pub struct DepositArgs {
    #[arg(short = 'b', long)]
    bucket: Option<String>,
    #[arg(short = 'a', long)]
    amount: Option<f64>,
}

#[derive(Args)]
pub struct WithdrawArgs {
    #[arg(short = 'b', long)]
    bucket: Option<String>,
    #[arg(short = 'a', long)]
    amount: Option<f64>,
}

pub fn run(args: BucketArgs) -> Result<()> {
    match args.command {
        BucketCommands::Create(ca) => create(ca),
        BucketCommands::List => list(),
        BucketCommands::Deposit(da) => deposit(da),
        BucketCommands::Withdraw(wa) => withdraw(wa),
    }
}

fn create(args: CreateArgs) -> Result<()> {
    let conn = open_db()?;

    let name = match args.name {
        Some(n) => n,
        None => helpers::map_dlg_err(Input::new().with_prompt("Bucket name").interact_text())?,
    };

    let existing = bucket_service::list_buckets(&conn)?
        .into_iter()
        .any(|b| b.name == name);
    if existing {
        eprintln!("Bucket '{name}' already exists");
        return Ok(());
    }

    let is_emergency = name.to_lowercase().contains("emergencia")
        || name.to_lowercase().contains("emergency");

    let bucket_type = if is_emergency {
        "emergency".to_string()
    } else {
        "target".to_string()
    };

    let target_amount = if bucket_type == "target" {
        match args.target {
            Some(t) => {
                if t <= 0.0 {
                    eprintln!("Target amount must be positive");
                    return Ok(());
                }
                Some(t)
            }
            None => {
                let t: f64 = helpers::map_dlg_err(
                    Input::new().with_prompt("Target amount ($)").interact_text(),
                )?;
                if t <= 0.0 {
                    eprintln!("Target amount must be positive");
                    return Ok(());
                }
                Some(t)
            }
        }
    } else {
        None
    };

    let bucket = Bucket {
        id: None,
        name: name.clone(),
        bucket_type,
        target_amount,
        savings_percentage: None,
        current_balance: 0.0,
    };

    bucket_service::create_bucket(&conn, &bucket)?;
    println!("✓ Bucket '{name}' created");

    Ok(())
}

fn list() -> Result<()> {
    let conn = open_db()?;
    let buckets = bucket_service::list_buckets(&conn)?;

    if buckets.is_empty() {
        println!("No buckets yet. Create one with `money-tracker bucket create`");
        return Ok(());
    }

    println!(
        "{:<25} {:<10} {:>12} {:>12} {:>10}",
        "Name", "Type", "Balance", "Target", "Progress"
    );
    println!("{}", "-".repeat(75));

    for b in &buckets {
        let progress = match b.progress_pct() {
            Some(p) => format!("{:.0}%", p),
            None => "—".to_string(),
        };
        let target = match b.target_amount {
            Some(t) => format!("${:.2}", t),
            None => "—".to_string(),
        };
        println!(
            "{:<25} {:<10} {:>12} {:>12} {:>10}",
            b.name,
            b.bucket_type,
            format!("${:.2}", b.current_balance),
            target,
            progress
        );
    }

    Ok(())
}

fn deposit(args: DepositArgs) -> Result<()> {
    let conn = open_db()?;
    let month = helpers::get_current_month();
    let year = helpers::get_current_year();
    let date = helpers::get_today();

    let bucket_name = match args.bucket {
        Some(b) => b,
        None => {
            let buckets = helpers::get_bucket_names(&conn)?;
            let selection = helpers::map_dlg_err(
                FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("Bucket")
                    .items(&buckets)
                    .default(0)
                    .interact(),
            )?;
            buckets[selection].clone()
        }
    };

    let bucket = bucket_service::get_bucket_by_name(&conn, &bucket_name)?;

    let amount = match args.amount {
        Some(a) => {
            if a <= 0.0 {
                eprintln!("Amount must be positive");
                return Ok(());
            }
            a
        }
        None => helpers::map_dlg_err(
            Input::new().with_prompt("Amount ($)").interact_text(),
        )?,
    };

    let desc: String = helpers::map_dlg_err(
        Input::new()
            .with_prompt("Description (optional)")
            .allow_empty(true)
            .interact_text(),
    )?;
    let description = if desc.is_empty() { None } else { Some(desc.as_str()) };

    bucket_service::deposit_to_bucket(
        &conn,
        bucket.id.unwrap(),
        amount,
        &date,
        description,
        month,
        year,
    )?;

    println!(
        "✓ ${:.2} deposited to '{}' (new balance: ${:.2})",
        amount,
        bucket.name,
        bucket.current_balance + amount
    );

    Ok(())
}

fn withdraw(args: WithdrawArgs) -> Result<()> {
    let conn = open_db()?;
    let month = helpers::get_current_month();
    let year = helpers::get_current_year();
    let date = helpers::get_today();

    let bucket_name = match args.bucket {
        Some(b) => b,
        None => {
            let buckets = helpers::get_bucket_names(&conn)?;
            let selection = helpers::map_dlg_err(
                FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("Bucket")
                    .items(&buckets)
                    .default(0)
                    .interact(),
            )?;
            buckets[selection].clone()
        }
    };

    let bucket = bucket_service::get_bucket_by_name(&conn, &bucket_name)?;

    let amount = match args.amount {
        Some(a) => {
            if a <= 0.0 {
                eprintln!("Amount must be positive");
                return Ok(());
            }
            a
        }
        None => helpers::map_dlg_err(
            Input::new().with_prompt("Amount ($)").interact_text(),
        )?,
    };

    let desc: String = helpers::map_dlg_err(
        Input::new()
            .with_prompt("Description (optional)")
            .allow_empty(true)
            .interact_text(),
    )?;
    let description = if desc.is_empty() { None } else { Some(desc.as_str()) };

    bucket_service::withdraw_from_bucket(
        &conn,
        bucket.id.unwrap(),
        amount,
        &date,
        description,
        month,
        year,
    )?;

    println!(
        "✓ ${:.2} withdrawn from '{}' (new balance: ${:.2})",
        amount,
        bucket.name,
        bucket.current_balance - amount
    );

    Ok(())
}
