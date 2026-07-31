use clap::Args;
use dialoguer::Input;
use money_core::db::open_db;
use money_core::services::entry_service;
use money_core::Result;

use crate::commands::helpers::{self, PromptMode};

#[derive(Args)]
pub struct TransferArgs {
    #[arg(short = 'a', long)]
    amount: Option<f64>,
    #[arg(short = 'f', long)]
    from: Option<String>,
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
}

pub fn run(args: TransferArgs) -> Result<()> {
    let conn = open_db()?;
    let any_given = args.amount.is_some() || args.from.is_some() || args.to.is_some();
    let mode = PromptMode::resolve(args.interactive, args.yes, any_given);

    let amount = match args.amount {
        Some(a) if a > 0.0 => a,
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
            eprintln!("Missing --amount");
            return Ok(());
        }
    };

    let from_name = match args.from {
        Some(f) => f,
        None if mode.allows_prompt() => {
            let names = helpers::get_account_names(&conn)?;
            let selection = helpers::map_dlg_err(
                dialoguer::FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("From account")
                    .items(&names)
                    .default(0)
                    .interact(),
            )?;
            names[selection].clone()
        }
        None => {
            eprintln!("Missing --from");
            return Ok(());
        }
    };
    let from = helpers::resolve_account(&conn, &from_name)?;

    let to_name = match args.to {
        Some(t) => t,
        None if mode.allows_prompt() => {
            let names = helpers::get_account_names(&conn)?;
            let selection = helpers::map_dlg_err(
                dialoguer::FuzzySelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("To account")
                    .items(&names)
                    .default(0)
                    .interact(),
            )?;
            names[selection].clone()
        }
        None => {
            eprintln!("Missing --to");
            return Ok(());
        }
    };
    let to = helpers::resolve_account(&conn, &to_name)?;

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

    entry_service::add_transfer(&conn, &date, amount, from.id, to.id, description.as_deref())?;

    println!(
        "✓ ${amount:.2} movido de '{}' a '{}' ({date})",
        from.name, to.name
    );
    Ok(())
}
