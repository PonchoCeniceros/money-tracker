use clap::{Args, Subcommand};
use dialoguer::{FuzzySelect, Input};
use money_core::db::open_db;
use money_core::period::Period;
use money_core::services::report_service;
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct BudgetArgs {
    #[command(subcommand)]
    command: BudgetCommands,
}

#[derive(Subcommand)]
enum BudgetCommands {
    Show(ShowArgs),
    Set(SetArgs),
    Rm(RmArgs),
}

#[derive(Args)]
pub struct ShowArgs {
    #[arg(short = 'p', long)]
    period: Option<String>,
}

#[derive(Args)]
pub struct SetArgs {
    #[arg(short = 'c', long)]
    concept: Option<String>,
    #[arg(short = 'l', long)]
    limit: Option<f64>,
    #[arg(short = 'p', long)]
    period: Option<String>,
}

#[derive(Args)]
pub struct RmArgs {
    #[arg(short = 'c', long)]
    concept: String,
    #[arg(short = 'p', long)]
    period: Option<String>,
}

pub fn run(args: BudgetArgs) -> Result<()> {
    match args.command {
        BudgetCommands::Show(a) => show(a),
        BudgetCommands::Set(a) => set(a),
        BudgetCommands::Rm(a) => rm(a),
    }
}

fn resolve_period(given: Option<String>) -> Result<Period> {
    match given {
        Some(p) => Period::parse(&p),
        None => Ok(Period::current()),
    }
}

fn show(args: ShowArgs) -> Result<()> {
    let conn = open_db()?;
    let period = resolve_period(args.period)?;

    // Budgets are informative only — this never blocks anything, it's
    // purely a read against the same accrued-expense join `report` uses.
    let report = report_service::monthly_report(&conn, &period)?;

    if report.budgets.is_empty() {
        println!("Sin presupuestos para {}", period.as_str());
        println!("Establece uno con `money-tracker budget set`");
        return Ok(());
    }

    println!("Presupuesto vs Real — {}", period.as_str());
    println!(
        "{:<25} {:>12} {:>12} {:>10}",
        "Concepto", "Presup.", "Real", "% Usado"
    );
    println!("{}", "-".repeat(65));

    for b in &report.budgets {
        println!(
            "{:<25} {:>12} {:>12} {:>9.0}%",
            b.concept,
            format!("${:.2}", b.budgeted),
            format!("${:.2}", b.actual),
            b.pct
        );
    }

    Ok(())
}

fn set(args: SetArgs) -> Result<()> {
    let conn = open_db()?;
    let period = resolve_period(args.period)?;

    let concept = match args.concept {
        Some(c) => helpers::resolve_concept(&conn, &c, "expense")?,
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

    let limit = match args.limit {
        Some(l) if l > 0.0 => l,
        Some(_) => {
            eprintln!("Limit must be positive");
            return Ok(());
        }
        None => helpers::map_dlg_err(
            Input::new()
                .with_prompt("Monthly limit ($)")
                .validate_with(|v: &f64| if *v > 0.0 { Ok(()) } else { Err("Limit must be positive") })
                .interact_text(),
        )?,
    };

    conn.execute(
        "INSERT INTO budgets (concept, monthly_limit, period) VALUES (?1, ?2, ?3)
         ON CONFLICT(concept, period) DO UPDATE SET monthly_limit = excluded.monthly_limit",
        rusqlite::params![concept, limit, period.as_str()],
    )?;

    println!(
        "✓ Presupuesto de '{concept}' para {}: ${limit:.2}/mes",
        period.as_str()
    );
    Ok(())
}

fn rm(args: RmArgs) -> Result<()> {
    let conn = open_db()?;
    let period = resolve_period(args.period)?;
    let affected = conn.execute(
        "DELETE FROM budgets WHERE concept = ?1 AND period = ?2",
        rusqlite::params![args.concept, period.as_str()],
    )?;
    if affected == 0 {
        println!("No había presupuesto de '{}' para {}", args.concept, period.as_str());
    } else {
        println!("✓ Presupuesto de '{}' para {} eliminado", args.concept, period.as_str());
    }
    Ok(())
}
