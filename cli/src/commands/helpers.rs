use chrono::{Duration, Local};
use money_core::error::AppError;
use money_core::models::AccountBalance;
use money_core::services::account_service;
use money_core::{period, Result};

/// Replaces the old all-or-nothing `interactive` heuristic (pre-redesign
/// `add.rs:33-39`), which gated *some* optional prompts on "every field is
/// absent" while leaving others (the concept picker) always prompting — so
/// `add -a 350` silently skipped `tipo`/description with no indication a
/// field was ever asked.
///
/// Three explicit modes, shared by every command that registers money:
/// - `Wizard`  — no args given, or `-i/--interactive`: prompt for everything,
///   optionals included.
/// - `Fill`    — some args given, no `-i`: prompt only for missing
///   *required* fields; optionals are silently `None` and side effects
///   (like the emergency-fund split) use their configured defaults.
/// - `Strict`  — `--yes`: never prompt; a missing required field is an
///   error, not a prompt. Makes the tool script/cron-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMode {
    Wizard,
    Fill,
    Strict,
}

impl PromptMode {
    pub fn resolve(interactive: bool, yes: bool, any_field_given: bool) -> PromptMode {
        if yes {
            PromptMode::Strict
        } else if interactive || !any_field_given {
            PromptMode::Wizard
        } else {
            PromptMode::Fill
        }
    }

    /// Whether an optional field (subconcept, description, ...) should be
    /// prompted for when absent.
    pub fn prompts_optionals(&self) -> bool {
        matches!(self, PromptMode::Wizard)
    }

    /// Whether a missing required field may be prompted for at all.
    pub fn allows_prompt(&self) -> bool {
        !matches!(self, PromptMode::Strict)
    }
}

/// Convert dialoguer errors to AppError
pub fn map_dlg_err<T>(r: std::result::Result<T, dialoguer::Error>) -> Result<T> {
    r.map_err(|e| AppError::Config(e.to_string()))
}

pub fn get_concept_names(conn: &rusqlite::Connection, type_filter: &str) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT name FROM concepts WHERE concept_type IN (?1, 'both') ORDER BY name")?;
    let rows = stmt.query_map(rusqlite::params![type_filter], |row| row.get::<_, String>(0))?;
    let mut names = Vec::new();
    for row in rows {
        names.push(row?);
    }
    Ok(names)
}

pub fn get_account_names(conn: &rusqlite::Connection) -> Result<Vec<String>> {
    Ok(account_service::list_accounts(conn, false)?
        .into_iter()
        .map(|a| a.name)
        .collect())
}

/// Resolves a user-typed account name against existing accounts:
/// exact case-insensitive match first, then a unique case-insensitive
/// prefix match. Ambiguous or absent matches error with the candidate list.
pub fn resolve_account(conn: &rusqlite::Connection, given: &str) -> Result<AccountBalance> {
    let accounts = account_service::list_accounts(conn, false)?;
    let needle = given.to_lowercase();

    if let Some(exact) = accounts.iter().find(|a| a.name.to_lowercase() == needle) {
        return Ok(exact.clone());
    }

    let prefix_matches: Vec<&AccountBalance> = accounts
        .iter()
        .filter(|a| a.name.to_lowercase().starts_with(&needle))
        .collect();

    match prefix_matches.len() {
        1 => Ok(prefix_matches[0].clone()),
        0 => Err(AppError::NotFound(format!(
            "No account matches '{given}'. Known accounts: {}",
            accounts
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
        _ => Err(AppError::Invalid(format!(
            "'{given}' matches multiple accounts: {}",
            prefix_matches
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Resolves a user-typed concept name the same way `resolve_account` does,
/// restricted to concepts usable for `type_filter` ("expense"/"income").
/// A typo like "Alimento" now fails loudly with a suggestion instead of
/// silently creating a new report row.
pub fn resolve_concept(conn: &rusqlite::Connection, given: &str, type_filter: &str) -> Result<String> {
    let candidates = get_concept_names(conn, type_filter)?;
    let needle = given.to_lowercase();

    if let Some(exact) = candidates.iter().find(|c| c.to_lowercase() == needle) {
        return Ok(exact.clone());
    }

    let prefix_matches: Vec<&String> = candidates
        .iter()
        .filter(|c| c.to_lowercase().starts_with(&needle))
        .collect();

    match prefix_matches.len() {
        1 => Ok(prefix_matches[0].clone()),
        0 => {
            let suggestion = candidates
                .iter()
                .min_by_key(|c| strsim_distance(&c.to_lowercase(), &needle));
            let hint = match suggestion {
                Some(s) => format!(" ¿Quisiste decir '{s}'? Usa `concept add` para crear uno nuevo."),
                None => " Usa `concept add` para crear uno nuevo.".to_string(),
            };
            Err(AppError::NotFound(format!(
                "Concepto '{given}' no existe.{hint}"
            )))
        }
        _ => Err(AppError::Invalid(format!(
            "'{given}' coincide con varios conceptos: {}",
            prefix_matches
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Minimal Levenshtein distance, just to pick a "did you mean" suggestion —
/// not exposed, not meant to be a general string-distance utility.
fn strsim_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Parses flexible date shorthand into canonical YYYY-MM-DD:
/// `None` / "hoy" / "today" -> today; "ayer" / "yesterday" -> yesterday;
/// a bare day-of-month (e.g. "15") -> that day in the current month;
/// "-N" -> N days ago; otherwise must already be YYYY-MM-DD.
pub fn parse_date(input: Option<&str>) -> Result<String> {
    let today = Local::now().date_naive();
    let s = match input {
        None => return Ok(period::today()),
        Some(s) => s.trim(),
    };

    match s {
        "hoy" | "today" => return Ok(today.format("%Y-%m-%d").to_string()),
        "ayer" | "yesterday" => {
            return Ok((today - Duration::days(1)).format("%Y-%m-%d").to_string())
        }
        _ => {}
    }

    if let Some(rest) = s.strip_prefix('-') {
        if let Ok(n) = rest.parse::<i64>() {
            return Ok((today - Duration::days(n)).format("%Y-%m-%d").to_string());
        }
    }

    if let Ok(day) = s.parse::<u32>() {
        if (1..=31).contains(&day) {
            use chrono::Datelike;
            if let Some(date) = today.with_day(day) {
                return Ok(date.format("%Y-%m-%d").to_string());
            }
        }
    }

    period::validate_date(s)?;
    Ok(s.to_string())
}
