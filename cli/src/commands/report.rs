use clap::Args;
use money_core::db::open_db;
use money_core::services::report_service;
use money_core::Result;
use tabled::settings::object::Rows;
use tabled::settings::{Alignment, Style};
use tabled::Table;
use tabled::Tabled;

use crate::commands::helpers::{get_current_month, get_current_year};

#[derive(Args)]
pub struct ReportArgs {
    /// Month (1-12)
    #[arg(short = 'm', long)]
    month: Option<i32>,
    /// Year
    #[arg(short = 'y', long)]
    year: Option<i32>,
}

#[derive(Tabled)]
struct ConceptRow {
    #[tabled(rename = "Concept")]
    concept: String,
    #[tabled(rename = "Spent")]
    spent: String,
    #[tabled(rename = "Budget")]
    budget: String,
    #[tabled(rename = "%")]
    pct: String,
    #[tabled(rename = "#")]
    count: i64,
}

pub fn run(args: ReportArgs) -> Result<()> {
    let conn = open_db()?;
    let month = args.month.unwrap_or_else(get_current_month);
    let year = args.year.unwrap_or_else(get_current_year);

    let status = report_service::full_status(&conn, month, year)?;
    let r = &status.report;

    println!(
        "╔══════════════════════════════════════╗"
    );
    println!(
        "║     MONTHLY REPORT  {:>2}/{}      ║",
        month, year
    );
    println!(
        "╚══════════════════════════════════════╝"
    );
    println!();

    println!("{:>25}: ${:.2}", "Total Income", r.total_income);
    println!("{:>25}: ${:.2}", "Total Expenses", r.total_expense);
    println!(
        "{:>25}: ${:.2}",
        "Net Flow",
        r.net_flow
    );
    println!(
        "{:>25}: ${:.2}",
        "Bucket Contributions", status.bucket_contributions
    );
    println!(
        "{:>25}: ${:.2}",
        "Bucket Withdrawals", status.bucket_withdrawals
    );
    println!(
        "{:>25}: ${:.2}",
        "Flujo (available)",
        status.flujo
    );
    println!();

    if !r.by_concept.is_empty() {
        println!("Expenses by Concept:");
        let mut rows = Vec::new();
        for c in &r.by_concept {
            let budget_info = r.budgets.iter().find(|b| b.concept == c.concept);
            let (budget_str, pct_str) = match budget_info {
                Some(b) => (format!("${:.2}", b.budgeted), format!("{:.0}%", b.pct)),
                None => ("—".into(), "—".into()),
            };
            rows.push(ConceptRow {
                concept: c.concept.clone(),
                spent: format!("${:.2}", c.total),
                budget: budget_str,
                pct: pct_str,
                count: c.count,
            });
        }

        let mut table = Table::new(rows);
        table.with(Style::ascii());
        table.modify(Rows::first(), Alignment::center_vertical());
        println!("{}", table);
        println!();
    }

    println!("Buckets:");
    if status.buckets.is_empty() {
        println!("  No buckets yet.");
    } else {
        for b in &status.buckets {
            let progress = match b.progress_pct() {
                Some(p) => format!(" ({:.0}%)", p),
                None => String::new(),
            };
            let target = match b.target_amount {
                Some(t) => format!(" / ${:.2}", t),
                None => String::new(),
            };
            println!(
                "  {:<25} ${:.2}{}{}",
                b.name, b.current_balance, target, progress
            );
        }
    }

    println!();
    println!(
        "Emergency fund rate: {:.0}% of income",
        r.emergency_pct
    );

    Ok(())
}
