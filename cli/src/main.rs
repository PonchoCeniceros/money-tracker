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
    /// Add an expense transaction
    Add(commands::add::AddArgs),
    /// Register an income transaction
    Income(commands::income::IncomeArgs),
    /// Manage savings buckets
    Bucket(commands::bucket::BucketArgs),
    /// Manage concepts
    Concept(commands::concept::ConceptArgs),
    /// Manage budgets
    Budget(commands::budget::BudgetArgs),
    /// Show monthly report
    Report(commands::report::ReportArgs),
    /// View/Set configuration
    Config(commands::config::ConfigArgs),
    /// Import transactions from ODS file
    Import(commands::import::ImportArgs),
    /// Set initial balances for flujo and buckets
    InitBalances(commands::init_balances::InitBalancesArgs),
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Add(args) => commands::add::run(args),
        Commands::Income(args) => commands::income::run(args),
        Commands::Bucket(args) => commands::bucket::run(args),
        Commands::Concept(args) => commands::concept::run(args),
        Commands::Budget(args) => commands::budget::run(args),
        Commands::Report(args) => commands::report::run(args),
        Commands::Config(args) => commands::config::run(args),
        Commands::Import(args) => commands::import::run(args),
        Commands::InitBalances(args) => commands::init_balances::run(args),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
