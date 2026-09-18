use clap::Args;
use dialoguer::{Confirm, FuzzySelect, Input};
use money_core::services::{account_service, entry_service};
use money_core::Result;

use crate::commands::helpers::{self, PromptMode};

#[derive(Args)]
pub struct IncomeArgs {
    amount: Option<f64>,
    concept: Option<String>,
    #[arg(short = 't', long)]
    to: Option<String>,
    #[arg(short = 'd', long)]
    description: Option<String>,
    #[arg(short = 'D', long)]
    date: Option<String>,
    #[arg(short = 'i', long)]
    interactive: bool,
    #[arg(long)]
    yes: bool,
    /// Skip emergency fund allocation entirely
    #[arg(long)]
    no_emergency: bool,
    /// Create the concept if it doesn't already exist
    #[arg(long = "new-concept")]
    new_concept: bool,
}

pub fn run(args: IncomeArgs) -> Result<()> {
    let be = helpers::backend()?;
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
            eprintln!("Missing amount. Usage: money-tracker income <AMOUNT> <CONCEPT>");
            return Ok(());
        }
    };

    let concept = match args.concept {
        Some(c) => match helpers::resolve_concept(&*be, &c, "income") {
            Ok(resolved) => resolved,
            Err(_) if args.new_concept => {
                be.add_concept(&c, "income")?;
                c
            }
            Err(e) => return Err(e),
        },
        None if mode.allows_prompt() => {
            let concepts = helpers::get_concept_names(&*be, "income")?;
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
            eprintln!("Missing concept. Usage: money-tracker income <AMOUNT> <CONCEPT>");
            return Ok(());
        }
    };

    let to = match args.to {
        Some(t) => helpers::resolve_account(&*be, &t)?,
        None => match be.get_config("income_account")? {
            Some(name) => helpers::resolve_account(&*be, &name)?,
            None => account_service::default_account(&*be)?,
        },
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

    let emergency_pct: f64 = be
        .get_config("emergency_pct")?
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(10.0);

    let emergency_account = account_service::emergency_account(&*be)?;

    let split = if args.no_emergency || emergency_account.is_none() || !to.liquid {
        false
    } else if mode == PromptMode::Wizard {
        helpers::map_dlg_err(
            Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt(format!(
                    "Allocate {emergency_pct:.0}% (${:.2}) to emergency fund?",
                    amount * emergency_pct / 100.0
                ))
                .default(true)
                .interact(),
        )?
    } else {
        true
    };

    let result = entry_service::add_income_with_emergency_split(
        &*be,
        &date,
        amount,
        to.id,
        &concept,
        description.as_deref(),
        split,
    )?;

    println!(
        "✓ Ingreso ${amount:.2} · {concept} · {} · {date}  (#{})",
        to.name, result.entry_id
    );

    match result.emergency {
        Some((fund_name, fund_amount)) => {
            println!("  → ${fund_amount:.2} a '{fund_name}' ({emergency_pct:.0}%)")
        }
        None if !to.liquid && emergency_account.is_some() => {
            println!("  (sin aporte a fondo: '{}' es una cuenta restringida)", to.name)
        }
        None => {}
    }

    Ok(())
}
