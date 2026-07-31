use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "money-tracker", version, about = "Personal finance tracker")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register an expense
    Add(commands::add::AddArgs),
    /// Register an income
    Income(commands::income::IncomeArgs),
    /// Move money between accounts (card payment, ATM withdrawal, ...)
    Transfer(commands::transfer::TransferArgs),
    /// Deposit into / withdraw from a savings bucket
    Bucket(commands::bucket::BucketArgs),
    /// Manage accounts (spending, emergency, target, credit)
    Account(commands::account::AccountArgs),
    /// List / delete individual entries
    Entry(commands::entry::EntryArgs),
    /// Manage concepts
    Concept(commands::concept::ConceptArgs),
    /// Manage budgets (informative only)
    Budget(commands::budget::BudgetArgs),
    /// Show monthly report
    Report(commands::report::ReportArgs),
    /// View/Set configuration
    Config(commands::config::ConfigArgs),
    /// Load opening balances into a fresh database
    Setup(commands::setup::SetupArgs),
    /// Inspect or reset the database file
    Db(commands::db::DbArgs),
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Add(args) => commands::add::run(args),
        Commands::Income(args) => commands::income::run(args),
        Commands::Transfer(args) => commands::transfer::run(args),
        Commands::Bucket(args) => commands::bucket::run(args),
        Commands::Account(args) => commands::account::run(args),
        Commands::Entry(args) => commands::entry::run(args),
        Commands::Concept(args) => commands::concept::run(args),
        Commands::Budget(args) => commands::budget::run(args),
        Commands::Report(args) => commands::report::run(args),
        Commands::Config(args) => commands::config::run(args),
        Commands::Setup(args) => commands::setup::run(args),
        Commands::Db(args) => commands::db::run(args),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
