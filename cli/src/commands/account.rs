use clap::{Args, Subcommand};
use dialoguer::{Input, Select};
use money_core::db::open_db;
use money_core::models::{AccountKind, NewAccount};
use money_core::services::account_service;
use money_core::Result;

use crate::commands::helpers::{self, PromptMode};

#[derive(Args)]
pub struct AccountArgs {
    #[command(subcommand)]
    command: AccountCommands,
}

#[derive(Subcommand)]
enum AccountCommands {
    /// Create an account (spending / emergency / target / credit)
    Add(AddArgs),
    /// List accounts with their derived balances
    List(ListArgs),
    /// Archive an account (refuses if its balance isn't zero)
    Archive(ArchiveArgs),
    /// Reconcile a spending account against a physically counted amount
    /// (the cash-envelope flow: writes the untracked expense or income)
    Reconcile(ReconcileArgs),
}

#[derive(Args)]
pub struct AddArgs {
    name: Option<String>,
    #[arg(long)]
    kind: Option<String>,
    #[arg(long)]
    target: Option<f64>,
    #[arg(long)]
    limit: Option<f64>,
    #[arg(long)]
    restricted: bool,
    #[arg(long)]
    yes: bool,
}

#[derive(Args)]
pub struct ListArgs {
    #[arg(long)]
    all: bool,
}

#[derive(Args)]
pub struct ArchiveArgs {
    name: Option<String>,
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
pub struct ReconcileArgs {
    name: Option<String>,
    #[arg(long)]
    actual: Option<f64>,
    #[arg(short = 'c', long)]
    concept: Option<String>,
    #[arg(short = 'D', long)]
    date: Option<String>,
}

pub fn run(args: AccountArgs) -> Result<()> {
    match args.command {
        AccountCommands::Add(a) => add(a),
        AccountCommands::List(a) => list(a),
        AccountCommands::Archive(a) => archive(a),
        AccountCommands::Reconcile(a) => reconcile(a),
    }
}

const KIND_OPTIONS: [&str; 4] = ["spending", "emergency", "target", "credit"];

fn add(args: AddArgs) -> Result<()> {
    let conn = open_db()?;
    let any_given = args.name.is_some() || args.kind.is_some();
    let mode = PromptMode::resolve(false, args.yes, any_given);

    let name = match args.name {
        Some(n) => n,
        None if mode.allows_prompt() => {
            helpers::map_dlg_err(Input::new().with_prompt("Account name").interact_text())?
        }
        None => {
            eprintln!("Missing account name. Usage: money-tracker account add <NAME> --kind <spending|emergency|target|credit>");
            return Ok(());
        }
    };

    let kind_str = match args.kind {
        Some(k) => k,
        None if mode.allows_prompt() => {
            let selection = helpers::map_dlg_err(
                Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("Kind")
                    .items(&KIND_OPTIONS)
                    .default(0)
                    .interact(),
            )?;
            KIND_OPTIONS[selection].to_string()
        }
        None => {
            eprintln!("Missing --kind <spending|emergency|target|credit>");
            return Ok(());
        }
    };
    let kind = AccountKind::from_str(&kind_str)?;

    let new_account = match kind {
        AccountKind::Spending => NewAccount::spending(&name),
        AccountKind::Emergency => NewAccount::emergency(&name),
        AccountKind::Target => {
            let target = match args.target {
                Some(t) => t,
                None if mode.allows_prompt() => helpers::map_dlg_err(
                    Input::new().with_prompt("Target amount ($)").interact_text(),
                )?,
                None => {
                    eprintln!("--target is required for a target account");
                    return Ok(());
                }
            };
            NewAccount::target(&name, target)?
        }
        AccountKind::Credit => {
            let limit = match args.limit {
                Some(l) => Some(l),
                None if mode.allows_prompt() => {
                    let l: String = helpers::map_dlg_err(
                        Input::new()
                            .with_prompt("Credit limit (optional, blank for none)")
                            .allow_empty(true)
                            .interact_text(),
                    )?;
                    if l.is_empty() {
                        None
                    } else {
                        Some(l.parse().map_err(|_| {
                            money_core::AppError::Invalid("Invalid credit limit".into())
                        })?)
                    }
                }
                None => None,
            };
            NewAccount::credit(&name, limit)
        }
    };

    let new_account = if args.restricted {
        new_account.restricted()
    } else {
        new_account
    };

    account_service::create_account(&conn, &new_account)?;
    println!("✓ Cuenta '{name}' creada ({kind_str})");
    Ok(())
}

fn list(args: ListArgs) -> Result<()> {
    let conn = open_db()?;
    let accounts = account_service::list_accounts(&conn, args.all)?;

    if accounts.is_empty() {
        println!("No accounts yet. Create one with `money-tracker account add`");
        return Ok(());
    }

    println!(
        "{:<25} {:<10} {:>14} {:<25}",
        "CUENTA", "TIPO", "SALDO", "META / LÍMITE"
    );
    println!("{}", "-".repeat(80));

    for a in &accounts {
        let extra = match a.kind {
            AccountKind::Target => match a.progress_pct() {
                Some(p) => format!(
                    "${:.2} ({:.0}%)",
                    a.target_amount.unwrap_or(0.0),
                    p
                ),
                None => "—".to_string(),
            },
            AccountKind::Credit => match a.available_credit() {
                Some(avail) => format!("deuda ${:.2} · disponible ${avail:.2}", a.debt()),
                None => format!("deuda ${:.2}", a.debt()),
            },
            _ => "—".to_string(),
        };
        let restricted = if !a.liquid { " (restringida)" } else { "" };
        let archived = if a.archived { " (archivada)" } else { "" };
        println!(
            "{:<25} {:<10} {:>14} {:<25}",
            format!("{}{restricted}{archived}", a.name),
            a.kind.as_str(),
            format!("${:.2}", a.balance),
            extra
        );
    }

    Ok(())
}

fn archive(args: ArchiveArgs) -> Result<()> {
    let conn = open_db()?;
    let name = match args.name {
        Some(n) => n,
        None => helpers::map_dlg_err(Input::new().with_prompt("Account name").interact_text())?,
    };
    let account = helpers::resolve_account(&conn, &name)?;
    account_service::archive_account(&conn, account.id, args.force)?;
    println!("✓ Cuenta '{}' archivada", account.name);
    Ok(())
}

fn reconcile(args: ReconcileArgs) -> Result<()> {
    let conn = open_db()?;
    let name = match args.name {
        Some(n) => n,
        None => helpers::map_dlg_err(Input::new().with_prompt("Account name").interact_text())?,
    };
    let account = helpers::resolve_account(&conn, &name)?;

    let actual = match args.actual {
        Some(a) => a,
        None => helpers::map_dlg_err(
            Input::new()
                .with_prompt(format!("Actual amount in '{}' ($)", account.name))
                .interact_text(),
        )?,
    };

    let default_concept = conn
        .query_row(
            "SELECT value FROM config WHERE key = 'cash_concept'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "Discrecional".to_string());

    let concept = match args.concept {
        Some(c) => c,
        None => default_concept,
    };
    let concept = helpers::resolve_concept(&conn, &concept, "expense")?;

    let date = helpers::parse_date(args.date.as_deref())?;

    let result = account_service::reconcile_account(&conn, account.id, actual, &concept, &date)?;

    match result.entry_id {
        None => println!("✓ Sin diferencia — '{}' ya está en ${actual:.2}", account.name),
        Some(_) if result.diff > 0.0 => println!(
            "✓ Cuadre: ${:.2} sin registrar en '{}' → gasto de ${:.2} · {concept}",
            result.diff, account.name, result.diff
        ),
        Some(_) => println!(
            "✓ Cuadre: ${:.2} de más en '{}' → ingreso de ${:.2} · {concept}",
            -result.diff, account.name, -result.diff
        ),
    }
    Ok(())
}
