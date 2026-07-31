use clap::Args;
use dialoguer::{FuzzySelect, Input};
use money_core::db::open_db;
use money_core::period::Period;
use money_core::services::{account_service, entry_service, report_service};
use money_core::Result;

use crate::commands::helpers::{self, PromptMode};

#[derive(Args)]
pub struct AddArgs {
    amount: Option<f64>,
    concept: Option<String>,
    #[arg(short = 'f', long)]
    from: Option<String>,
    #[arg(short = 's', long)]
    subconcept: Option<String>,
    #[arg(short = 'd', long)]
    description: Option<String>,
    #[arg(short = 'D', long)]
    date: Option<String>,
    #[arg(short = 'i', long)]
    interactive: bool,
    #[arg(long)]
    yes: bool,
    /// Create the concept if it doesn't already exist
    #[arg(long = "new-concept")]
    new_concept: bool,
}

pub fn run(args: AddArgs) -> Result<()> {
    let conn = open_db()?;
    let any_given = args.amount.is_some() || args.concept.is_some();
    let mode = PromptMode::resolve(args.interactive, args.yes, any_given);

    let amount = match args.amount {
        Some(v) if v > 0.0 => v,
        Some(_) => {
            eprintln!("Amount must be positive");
            return Ok(());
        }
        None if mode.allows_prompt() => helpers::map_dlg_err(
            Input::new()
                .with_prompt("Amount ($)")
                .validate_with(|v: &f64| if *v > 0.0 { Ok(()) } else { Err("Amount must be positive") })
                .interact_text(),
        )?,
        None => {
            eprintln!("Missing amount. Usage: money-tracker add <AMOUNT> <CONCEPT>");
            return Ok(());
        }
    };

    let concept = match args.concept {
        Some(c) => match helpers::resolve_concept(&conn, &c, "expense") {
            Ok(resolved) => resolved,
            Err(_) if args.new_concept => {
                conn.execute(
                    "INSERT INTO concepts (name, concept_type) VALUES (?1, 'expense')",
                    rusqlite::params![c],
                )?;
                c
            }
            Err(e) => return Err(e),
        },
        None if mode.allows_prompt() => {
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
        None => {
            eprintln!("Missing concept. Usage: money-tracker add <AMOUNT> <CONCEPT>");
            return Ok(());
        }
    };

    let from = match args.from {
        Some(f) => helpers::resolve_account(&conn, &f)?,
        None => account_service::default_account(&conn)?,
    };

    let subconcept = match args.subconcept {
        Some(s) => Some(s),
        None if mode.prompts_optionals() => {
            let s: String = helpers::map_dlg_err(
                Input::new()
                    .with_prompt("Subconcept (optional)")
                    .allow_empty(true)
                    .interact_text(),
            )?;
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        None => None,
    };

    let description = match args.description {
        Some(d) => Some(d),
        None if mode.prompts_optionals() => {
            let d: String = helpers::map_dlg_err(
                Input::new()
                    .with_prompt("Description (optional)")
                    .allow_empty(true)
                    .interact_text(),
            )?;
            if d.is_empty() {
                None
            } else {
                Some(d)
            }
        }
        None => None,
    };

    let date = helpers::parse_date(args.date.as_deref())?;

    let entry_id = entry_service::add_expense(
        &conn,
        &date,
        amount,
        from.id,
        &concept,
        subconcept.as_deref(),
        description.as_deref(),
    )?;

    println!(
        "✓ Gasto ${amount:.2} · {concept} · {} · {date}  (#{entry_id})",
        from.name
    );

    // Budgets are informative only: warn on overrun, never block.
    if let Ok(period) = Period::from_date(&date) {
        if let Ok(report) = report_service::monthly_report(&conn, &period) {
            if let Some(budget) = report.budgets.iter().find(|b| b.concept == concept) {
                if budget.pct > 100.0 {
                    println!(
                        "  ⚠ {concept}: ${:.2} de ${:.2} presupuestado ({:.0}%)",
                        budget.actual, budget.budgeted, budget.pct
                    );
                }
            }
        }
    }

    Ok(())
}
