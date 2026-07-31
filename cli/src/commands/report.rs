use clap::Args;
use money_core::db::open_db;
use money_core::period::Period;
use money_core::services::report_service;
use money_core::Result;
use tabled::settings::object::Rows;
use tabled::settings::{Alignment, Style};
use tabled::Table;
use tabled::Tabled;

#[derive(Args)]
pub struct ReportArgs {
    /// Period, "YYYY-MM" (default: current month)
    #[arg(short = 'p', long)]
    period: Option<String>,
    /// Show the accrued/cash breakdown and internal transfers
    #[arg(long)]
    detail: bool,
}

#[derive(Tabled)]
struct ConceptRow {
    #[tabled(rename = "Concepto")]
    concept: String,
    #[tabled(rename = "Gastado")]
    spent: String,
    #[tabled(rename = "Presup.")]
    budget: String,
    #[tabled(rename = "%")]
    pct: String,
    #[tabled(rename = "#")]
    count: i64,
}

pub fn run(args: ReportArgs) -> Result<()> {
    let conn = open_db()?;
    let period = match args.period {
        Some(p) => Period::parse(&p)?,
        None => Period::current(),
    };

    let status = report_service::full_status(&conn, &period)?;
    let r = &status.report;
    let nw = &status.net_worth;

    println!("╔══════════════════════════════════════╗");
    println!("║     REPORTE MENSUAL   {:<15}║", period.as_str());
    println!("╚══════════════════════════════════════╝");
    println!();

    // The two numbers: what you consumed (accrued, what budgets compare
    // against) vs. what actually left your spending accounts (cash).
    // With a credit card or an expense funded from savings, these
    // genuinely differ — see the "cash envelope" / credit-card note in
    // the plan for why neither one alone tells the whole story.
    println!("{:>28}: ${:.2}", "Gasto del mes (devengado)", r.total_expense);
    if args.detail {
        let paid_with_flow = r.total_expense - r.from_savings - r.on_credit;
        println!("{:>28}  {:.2}", "  pagado con flujo del mes", paid_with_flow);
        println!("{:>28}  {:.2}", "  financiado con ahorro", r.from_savings);
        println!("{:>28}  {:.2}", "  a crédito", r.on_credit);
    }
    println!("{:>28}: ${:.2}", "Salida real de efectivo", r.cash_out);
    println!();
    println!("{:>28}: ${:.2}", "Ingreso del mes", r.total_income);
    println!("{:>28}: ${:.2}", "Flujo neto (devengado)", r.net_flow);

    if r.savings_contributions > 0.0 {
        println!("{:>28}: ${:.2}", "Aportes a ahorro", r.savings_contributions);
    }
    if r.savings_withdrawals > 0.0 {
        println!("{:>28}: ${:.2}", "Retiros de ahorro", r.savings_withdrawals);
    }
    if r.card_payments > 0.0 {
        println!("{:>28}: ${:.2}", "Pagos de tarjeta", r.card_payments);
    }
    println!();

    if !r.by_concept.is_empty() || !r.budgets.is_empty() {
        println!("Gastos por concepto:");
        let mut rows = Vec::new();
        let mut shown = std::collections::HashSet::new();
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
            shown.insert(c.concept.as_str());
        }
        for b in &r.budgets {
            if !shown.contains(b.concept.as_str()) {
                rows.push(ConceptRow {
                    concept: b.concept.clone(),
                    spent: format!("${:.2}", 0.0),
                    budget: format!("${:.2}", b.budgeted),
                    pct: format!("{:.0}%", b.pct),
                    count: 0,
                });
            }
        }

        let mut table = Table::new(rows);
        table.with(Style::ascii());
        table.modify(Rows::first(), Alignment::center_vertical());
        println!("{}", table);
        println!();
    }

    println!("Cuentas:");
    if nw.accounts.is_empty() {
        println!("  No hay cuentas todavía. Créalas con `money-tracker account add`.");
    } else {
        for a in &nw.accounts {
            let extra = match a.kind {
                money_core::models::AccountKind::Target => match a.progress_pct() {
                    Some(p) => format!(" / ${:.2} ({:.0}%)", a.target_amount.unwrap_or(0.0), p),
                    None => String::new(),
                },
                money_core::models::AccountKind::Credit => match a.available_credit() {
                    Some(avail) => format!(" (deuda ${:.2} · disponible ${avail:.2})", a.debt()),
                    None => format!(" (deuda ${:.2})", a.debt()),
                },
                _ => String::new(),
            };
            println!("  {:<25} ${:.2}{extra}", a.name, a.balance);
        }
    }

    println!();
    println!("{:>20}: ${:.2}", "Efectivo disponible", nw.cash_on_hand);
    println!("{:>20}: ${:.2}", "Ahorro", nw.savings);
    println!("{:>20}: ${:.2}", "Deuda de tarjeta", nw.credit_debt);
    println!("{:>20}: ${:.2}", "Patrimonio neto", nw.net);

    Ok(())
}
