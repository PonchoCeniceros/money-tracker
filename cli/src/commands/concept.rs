use clap::{Args, Subcommand};
use dialoguer::Input;
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
    let be = helpers::backend()?;
    let concepts = money_core::services::concept_service::list(&*be, None)?;

    println!("{:<5} {:<25} {:<10}", "ID", "Name", "Type");
    println!("{}", "-".repeat(45));
    for c in &concepts {
        println!("{:<5} {:<25} {:<15}", c.id.unwrap_or(0), c.name, c.concept_type);
    }

    Ok(())
}

fn add(args: AddConceptArgs) -> Result<()> {
    let be = helpers::backend()?;

    let name = match args.name {
        Some(n) => n,
        None => helpers::map_dlg_err(Input::new().with_prompt("Concept name").interact_text())?,
    };

    let concept_type = args.concept_type.clone();

    money_core::services::concept_service::add(&*be, &name, &concept_type)?;

    println!("✓ Concept '{name}' added");
    Ok(())
}