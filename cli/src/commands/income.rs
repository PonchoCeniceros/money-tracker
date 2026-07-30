use clap::Args;
use dialoguer::{Confirm, FuzzySelect, Input};
use money_core::db::open_db;
use money_core::models::Transaction;
use money_core::services::{bucket_service, transaction_service};
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct IncomeArgs {
    #[arg(short = 'a', long)]
    amount: Option<f64>,
    #[arg(short = 'c', long)]
    concept: Option<String>,
    #[arg(short = 'd', long)]
    description: Option<String>,
    #[arg(short = 'm', long)]
    month: Option<i32>,
    #[arg(short = 'y', long)]
    year: Option<i32>,
}

pub fn run(args: IncomeArgs) -> Result<()> {
    let conn = open_db()?;
    let month = args.month.unwrap_or_else(helpers::get_current_month);
    let year = args.year.unwrap_or_else(helpers::get_current_year);
    let date = helpers::get_today();

    let amount = match args.amount {
        Some(v) => {
            if v <= 0.0 {
                eprintln!("Amount must be positive");
                return Ok(());
            }
            v
        }
        None => helpers::map_dlg_err(
            Input::new().with_prompt("Amount ($)").interact_text(),
        )?,
    };

    let concept = match args.concept {
        Some(c) => c,
        None => {
            let concepts = helpers::get_concept_names(&conn, "income")?;
            let selection = helpers::map_dlg_err(
                FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("Concept")
                    .items(&concepts)
                    .default(0)
                    .interact(),
            )?;
            concepts[selection].clone()
        }
    };

    let description = match args.description {
        Some(d) => Some(d),
        None => {
            let d: String = helpers::map_dlg_err(
                Input::new()
                    .with_prompt("Description (optional)")
                    .allow_empty(true)
                    .interact_text(),
            )?;
            if d.is_empty() { None } else { Some(d) }
        }
    };

    let txn = Transaction {
        id: None,
        date: date.clone(),
        amount,
        concept,
        subconcept: None,
        tipo: None,
        description,
        month,
        year,
    };

    transaction_service::add_income(&conn, &txn)?;
    println!("✓ Income registered: ${:.2}", amount);

    let emergency_pct: f64 = conn
        .query_row(
            "SELECT value FROM config WHERE key = 'emergency_pct'",
            [],
            |row| {
                let v: String = row.get(0)?;
                Ok(v.parse::<f64>().unwrap_or(10.0))
            },
        )
        .unwrap_or(10.0);

    if helpers::map_dlg_err(
        Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
            .with_prompt(format!(
                "Allocate {:.0}% (${:.2}) to emergency fund?",
                emergency_pct,
                amount * emergency_pct / 100.0
            ))
            .default(true)
            .interact(),
    )? {
        let emergency_amount = amount * emergency_pct / 100.0;
        let buckets = bucket_service::list_buckets(&conn)?;
        let emergency_bucket = buckets.iter().find(|b| b.bucket_type == "emergency");

        match emergency_bucket {
            Some(b) => {
                bucket_service::deposit_to_bucket(
                    &conn,
                    b.id.unwrap(),
                    emergency_amount,
                    &date,
                    Some("Auto: emergency fund allocation"),
                    month,
                    year,
                )?;
                println!("  → ${:.2} allocated to '{}'", emergency_amount, b.name);
            }
            None => {
                println!("  No emergency bucket found. Create one with `money-tracker bucket create`");
            }
        }
    }

    Ok(())
}
