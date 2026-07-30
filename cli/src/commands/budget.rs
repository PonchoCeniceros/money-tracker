use clap::{Args, Subcommand};
use dialoguer::{FuzzySelect, Input};
use money_core::db::open_db;
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
}

#[derive(Args)]
pub struct ShowArgs {
    #[arg(short = 'm', long)]
    month: Option<i32>,
    #[arg(short = 'y', long)]
    year: Option<i32>,
}

#[derive(Args)]
pub struct SetArgs {
    #[arg(short = 'c', long)]
    concept: Option<String>,
    #[arg(short = 'l', long)]
    limit: Option<f64>,
    #[arg(short = 'm', long)]
    month: Option<i32>,
    #[arg(short = 'y', long)]
    year: Option<i32>,
}

pub fn run(args: BudgetArgs) -> Result<()> {
    match args.command {
        BudgetCommands::Show(sa) => show(sa),
        BudgetCommands::Set(sa) => set(sa),
    }
}

fn show(args: ShowArgs) -> Result<()> {
    let conn = open_db()?;
    let month = args.month.unwrap_or_else(helpers::get_current_month);
    let year = args.year.unwrap_or_else(helpers::get_current_year);

    let mut stmt = conn.prepare(
        "SELECT concept, monthly_limit FROM budgets WHERE month = ?1 AND year = ?2 ORDER BY concept",
    )?;
    let rows = stmt.query_map(rusqlite::params![month, year], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })?;

    let mut budgets = Vec::new();
    for row in rows {
        let (concept, limit) = row?;
        let actual: f64 = conn.query_row(
            "SELECT COALESCE(SUM(ABS(amount)), 0) FROM transactions
             WHERE amount < 0 AND concept = ?1 AND month = ?2 AND year = ?3",
            rusqlite::params![concept, month, year],
            |row| row.get(0),
        )?;
        let pct = if limit > 0.0 {
            (actual / limit * 100.0).min(999.0)
        } else {
            0.0
        };
        budgets.push((concept, limit, actual, pct));
    }

    if budgets.is_empty() {
        println!("No budgets set for {}/{}", month, year);
        println!("Set one with `money-tracker budget set`");
        return Ok(());
    }

    println!("Budget vs Actual — {}/{}", month, year);
    println!(
        "{:<25} {:>12} {:>12} {:>10}",
        "Concept", "Budget", "Actual", "% Used"
    );
    println!("{}", "-".repeat(65));

    for (concept, limit, actual, pct) in &budgets {
        println!(
            "{:<25} {:>12} {:>12} {:>9.0}%",
            concept,
            format!("${:.2}", limit),
            format!("${:.2}", actual),
            pct
        );
    }

    Ok(())
}

fn set(args: SetArgs) -> Result<()> {
    let conn = open_db()?;
    let month = args.month.unwrap_or_else(helpers::get_current_month);
    let year = args.year.unwrap_or_else(helpers::get_current_year);

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

    let limit = match args.limit {
        Some(l) => l,
        None => {
            helpers::map_dlg_err(
                Input::new().with_prompt("Monthly limit ($)").interact_text(),
            )?
        }
    };

    conn.execute(
        "DELETE FROM budgets WHERE concept = ?1 AND month = ?2 AND year = ?3",
        rusqlite::params![concept, month, year],
    )?;
    conn.execute(
        "INSERT INTO budgets (concept, monthly_limit, month, year) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![concept, limit, month, year],
    )?;

    println!("✓ Budget for '{concept}' set to ${:.2}/month", limit);
    Ok(())
}
