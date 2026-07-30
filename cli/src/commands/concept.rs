use clap::{Args, Subcommand};
use dialoguer::Input;
use money_core::db::open_db;
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct ConceptArgs {
    #[command(subcommand)]
    command: ConceptCommands,
}

#[derive(Subcommand)]
enum ConceptCommands {
    List,
    Add(AddConceptArgs),
}

#[derive(Args)]
pub struct AddConceptArgs {
    name: Option<String>,
    #[arg(short = 't', long, default_value = "both")]
    concept_type: String,
}

pub fn run(args: ConceptArgs) -> Result<()> {
    match args.command {
        ConceptCommands::List => list(),
        ConceptCommands::Add(ca) => add(ca),
    }
}

fn list() -> Result<()> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT id, name, concept_type FROM concepts ORDER BY concept_type, name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    println!("{:<5} {:<25} {:<10}", "ID", "Name", "Type");
    println!("{}", "-".repeat(45));
    for row in rows {
        let (id, name, ctype) = row?;
        println!("{:<5} {:<25} {:<10}", id, name, ctype);
    }

    Ok(())
}

fn add(args: AddConceptArgs) -> Result<()> {
    let conn = open_db()?;

    let name = match args.name {
        Some(n) => n,
        None => helpers::map_dlg_err(Input::new().with_prompt("Concept name").interact_text())?,
    };

    let concept_type = args.concept_type.clone();
    if !["expense", "income", "both"].contains(&concept_type.as_str()) {
        eprintln!("Type must be: expense, income, or both");
        return Ok(());
    }

    conn.execute(
        "INSERT INTO concepts (name, concept_type) VALUES (?1, ?2)",
        rusqlite::params![name, concept_type],
    )?;

    println!("✓ Concept '{name}' added");
    Ok(())
}
