use clap::{Args, Subcommand};
use money_core::db::open_db;
use money_core::Result;

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
    let conn = open_db()?;
    let value: std::result::Result<String, _> = conn.query_row(
        "SELECT value FROM config WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get(0),
    );

    match value {
        Ok(v) => println!("{} = {}", key, v),
        Err(_) => eprintln!("Config key '{}' not found", key),
    }

    Ok(())
}

fn set(key: String, value: String) -> Result<()> {
    let conn = open_db()?;
    conn.execute(
        "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )?;
    println!("✓ {} = {}", key, value);
    Ok(())
}

fn list() -> Result<()> {
    let conn = open_db()?;
    let mut stmt = conn.prepare("SELECT key, value FROM config ORDER BY key")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    println!("{:<30} Value", "Key");
    println!("{}", "-".repeat(45));
    for row in rows {
        let (k, v) = row?;
        println!("{:<30} {}", k, v);
    }

    Ok(())
}
