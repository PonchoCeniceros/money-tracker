use clap::Args;
use dialoguer::{FuzzySelect, Input, Select};
use money_core::db::open_db;
use money_core::models::Transaction;
use money_core::services::transaction_service;
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct AddArgs {
    #[arg(short = 'a', long)]
    amount: Option<f64>,
    #[arg(short = 'c', long)]
    concept: Option<String>,
    #[arg(short = 's', long)]
    subconcept: Option<String>,
    #[arg(short = 't', long)]
    tipo: Option<String>,
    #[arg(short = 'd', long)]
    description: Option<String>,
    #[arg(short = 'm', long)]
    month: Option<i32>,
    #[arg(short = 'y', long)]
    year: Option<i32>,
}

pub fn run(args: AddArgs) -> Result<()> {
    let conn = open_db()?;
    let month = args.month.unwrap_or_else(helpers::get_current_month);
    let year = args.year.unwrap_or_else(helpers::get_current_year);

    let amount = match args.amount {
        Some(v) => {
            if v <= 0.0 {
                eprintln!("Amount must be positive");
                return Ok(());
            }
            -v
        }
        None => {
            let v: f64 = helpers::map_dlg_err(
                Input::new()
                    .with_prompt("Amount ($)")
                    .validate_with(|v: &f64| {
                        if *v <= 0.0 {
                            Err("Amount must be positive")
                        } else {
                            Ok(())
                        }
                    })
                    .interact_text(),
            )?;
            -v
        }
    };

    let concept = match args.concept {
        Some(c) => c,
        None => {
            let concepts = helpers::get_concept_names(&conn, "expense")?;
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

    let subconcept = match args.subconcept {
        Some(s) => Some(s),
        None => {
            let s: String = helpers::map_dlg_err(
                Input::new()
                    .with_prompt("Subconcept (optional)")
                    .allow_empty(true)
                    .interact_text(),
            )?;
            if s.is_empty() { None } else { Some(s) }
        }
    };

    let tipo = match args.tipo {
        Some(t) => Some(t),
        None => {
            let tipos = vec!["Liquido", "Credito", "Despensa", "Fondo emergencia"];
            let selection = helpers::map_dlg_err(
                Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("Type")
                    .items(&tipos)
                    .default(0)
                    .interact(),
            )?;
            Some(tipos[selection].to_string())
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
        date: helpers::get_today(),
        amount,
        concept,
        subconcept,
        tipo,
        description,
        month,
        year,
    };

    transaction_service::add_transaction(&conn, &txn)?;
    println!("✓ Expense registered: ${:.2}", amount.abs());
    Ok(())
}
