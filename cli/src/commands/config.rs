use clap::{Args, Subcommand};
use money_core::Result;

use crate::commands::helpers;

#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommands,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Get a config value
    Get { key: String },
    /// Set a config value
    Set { key: String, value: String },
    /// List all config
    List,
}

pub fn run(args: ConfigArgs) -> Result<()> {
    match args.command {
        ConfigCommands::Get { key } => get(key),
        ConfigCommands::Set { key, value } => set(key, value),
        ConfigCommands::List => list(),
    }
}

fn get(key: String) -> Result<()> {
    let be = helpers::backend()?;
    match be.get_config(&key)? {
        Some(v) => println!("{} = {}", key, v),
        None => eprintln!("Config key '{}' not found", key),
    }
    Ok(())
}

fn set(key: String, value: String) -> Result<()> {
    let be = helpers::backend()?;
    be.set_config(&key, &value)?;
    println!("✓ {} = {}", key, value);
    Ok(())
}

fn list() -> Result<()> {
    let be = helpers::backend()?;
    let entries = be.list_config()?;

    println!("{:<30} Value", "Key");
    println!("{}", "-".repeat(45));
    for e in entries {
        println!("{:<30} {}", e.key, e.value);
    }

    Ok(())
}