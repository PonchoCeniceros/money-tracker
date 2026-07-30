use std::io::Read;

use chrono::Datelike;
use chrono::NaiveDate;
use quick_xml::events::Event;
use quick_xml::Reader;
use rusqlite::Connection;
use zip::ZipArchive;

use crate::error::{AppError, Result};
use crate::models::Transaction;
use crate::services::transaction_service;

pub fn import_ods(conn: &Connection, path: &str) -> Result<ImportSummary> {
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut content_xml = String::new();
    let mut found = false;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.name() == "content.xml" {
            entry.read_to_string(&mut content_xml)?;
            found = true;
            break;
        }
    }

    if !found {
        return Err(AppError::Import("content.xml not found in ODS".into()));
    }

    let txns = parse_content_xml(&content_xml)?;
    let mut total_imported = 0;
    let mut concepts_found = std::collections::HashSet::new();

    conn.execute_batch("BEGIN TRANSACTION")?;

    for t in &txns {
        transaction_service::add_transaction(conn, t)?;
        concepts_found.insert(t.concept.clone());
        total_imported += 1;
    }

    for concept in &concepts_found {
        conn.execute(
            "INSERT OR IGNORE INTO concepts (name, concept_type) VALUES (?1, 'both')",
            rusqlite::params![concept],
        )?;
    }

    conn.execute_batch("COMMIT")?;

    Ok(ImportSummary {
        total_transactions: total_imported,
        concepts: concepts_found.into_iter().collect(),
        sheets_processed: vec!["ABRIL".into(), "MAYO".into(), "JUNIO".into(), "JULIO".into()],
    })
}

#[derive(Debug)]
pub struct ImportSummary {
    pub total_transactions: usize,
    pub concepts: Vec<String>,
    pub sheets_processed: Vec<String>,
}

fn parse_content_xml(xml: &str) -> Result<Vec<Transaction>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut txns = Vec::new();
    let mut current_row_cells: Vec<String> = Vec::new();
    let mut in_p = false;
    let mut cell_text = String::new();
    let mut section_detected = false;
    let mut month = 0i32;
    let year = 2026i32;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag.as_str() {
                    "table:table" => {
                        let table_name = e
                            .attributes()
                            .filter_map(|a| {
                                let a = a.ok()?;
                                if a.key.as_ref() == b"table:name" {
                                    String::from_utf8(a.value.to_vec()).ok()
                                } else {
                                    None
                                }
                            })
                            .next()
                            .unwrap_or_default();
                        current_row_cells.clear();
                        section_detected = false;
                        month = match table_name.as_str() {
                            "ABRIL" => 4,
                            "MAYO" => 5,
                            "JUNIO" => 6,
                            "JULIO" => 7,
                            _ => 0,
                        };
                    }
                    "text:p" => {
                        in_p = true;
                        cell_text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_p {
                    if let Ok(text) = e.unescape() {
                        cell_text.push_str(&text);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag.as_str() {
                    "text:p" => {
                        in_p = false;
                    }
                    "table:table-cell" => {
                        current_row_cells.push(cell_text.clone());
                        cell_text.clear();
                    }
                    "table:table-row" => {
                        if !current_row_cells.is_empty()
                            && section_detected
                            && month >= 4
                            && month <= 7
                        {
                            // Scan all cells for valid (amount, date, concept) groups
                            let mut idx = 0;
                            while idx + 2 < current_row_cells.len() {
                                if let Some(txn) =
                                    parse_row(&current_row_cells, idx, month, year)
                                {
                                    txns.push(txn);
                                    // Skip 3 cells (amount, date, concept are minimum)
                                    idx += 3;
                                } else {
                                    idx += 1;
                                }
                            }
                        }
                        let joined: String = current_row_cells.join(" | ");
                        if joined.contains("monto")
                            && (joined.contains("concepto") || joined.contains("concept"))
                            && (joined.contains("subconcepto") || joined.contains("tipo"))
                        {
                            section_detected = true;
                        }
                        current_row_cells.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(AppError::Import(format!("XML parse error: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(txns)
}

fn parse_row(cells: &[String], idx: usize, month: i32, year: i32) -> Option<Transaction> {
    if idx + 2 >= cells.len() {
        return None;
    }

    // Check if cells[idx] is a valid dollar amount
    let val = cells[idx].replace(['$', ','], "").trim().to_string();
    if val.is_empty() {
        return None;
    }
    let amount: f64 = val.parse().ok()?;
    if amount == 0.0 {
        return None;
    }

    // Check if cells[idx+1] is a valid date
    let date_str = cells[idx + 1].trim();
    if date_str.is_empty() {
        return None;
    }
    let date = parse_date(date_str, month, year)?;

    // Check if cells[idx+2] is a non-empty concept
    let concept = cells[idx + 2].trim().to_string();
    if concept.is_empty() || concept == "0" || concept == "$0.00" {
        return None;
    }
    // Skip if concept looks like a raw number (parser picking up wrong columns)
    if concept.parse::<f64>().is_ok() {
        return None;
    }

    let (tx_month, tx_year) = if let Ok(d) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") {
        (d.month() as i32, d.year())
    } else {
        (month, year)
    };

    let tipos = ["Liquido", "Credito", "Despensa", "Emergencia"];

    // Detect fund movement: if concept is a tipo and next cell has a cuenta name
    let next_has_cuenta = cells
        .get(idx + 3)
        .map(|s| !s.trim().is_empty() && !s.trim().chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false);
    let is_fund_movement = tipos.contains(&concept.as_str()) && next_has_cuenta;

    if is_fund_movement {
        // Fund movement: use cuenta as concept, tipo as subconcept
        let cuenta = cells[idx + 3].trim().to_string();
        let desc = cells.get(idx + 4).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        return Some(Transaction {
            id: None,
            date,
            amount, // already signed from original data
            concept: cuenta,
            subconcept: Some(concept),
            tipo: Some("Fondo".to_string()),
            description: desc,
            month: tx_month,
            year: tx_year,
        });
    }

    // Voluntario and Patronal are fund movements (savings contribs/withdrawals)
    let is_saving_concept = concept == "Voluntario" || concept == "Patronal" || concept == "Ahorro Patronal";
    if is_saving_concept {
        let desc = cells.get(idx + 3).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        return Some(Transaction {
            id: None,
            date,
            amount,
            concept,
            subconcept: None,
            tipo: Some("Fondo".to_string()),
            description: desc,
            month: tx_month,
            year: tx_year,
        });
    }

    let income_concepts = [
        "Nomina", "Saldo inicial",
    ];
    let is_income = income_concepts.iter().any(|c| concept.contains(c))
        || concept.contains("Ingreso")
        || concept == "Despensa"
        || concept == "Extraordinario";

    let subconcept = cells
        .get(idx + 3)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let tipo = cells
        .get(idx + 4)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let description = cells
        .get(idx + 5)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let final_amount = if is_income { amount } else { -amount };
    Some(Transaction {
        id: None,
        date,
        amount: final_amount,
        concept,
        subconcept,
        tipo,
        description,
        month: tx_month,
        year: tx_year,
    })
}

fn parse_date(s: &str, default_month: i32, default_year: i32) -> Option<String> {
    if let Ok(d) = NaiveDate::parse_from_str(s, "%d/%m/%Y") {
        return Some(d.format("%Y-%m-%d").to_string());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%-d/%-m/%Y") {
        return Some(d.format("%Y-%m-%d").to_string());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%d/%m/%y") {
        return Some(d.format("%Y-%m-%d").to_string());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.format("%Y-%m-%d").to_string());
    }
    if let Ok(day) = s.parse::<i32>() {
        if (1..=31).contains(&day) {
            let m = default_month.min(12).max(1);
            let y = default_year;
            if let Some(d) = NaiveDate::from_ymd_opt(y, m as u32, day as u32) {
                return Some(d.format("%Y-%m-%d").to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_date_formats() {
        assert_eq!(
            parse_date("4/7/2026", 7, 2026).as_deref(),
            Some("2026-07-04")
        );
        assert_eq!(
            parse_date("10/7/2026", 7, 2026).as_deref(),
            Some("2026-07-10")
        );
        assert_eq!(
            parse_date("2026-07-04", 7, 2026).as_deref(),
            Some("2026-07-04")
        );
    }

    #[test]
    fn test_parse_row_expense() {
        let cells: Vec<String> = vec![
            "$200.00", "4/7/2026", "Transporte", "combustible", "Liquido", "",
        ].into_iter().map(String::from).collect();
        let txn = parse_row(&cells, 0, 7, 2026).unwrap();
        assert_eq!(txn.amount, -200.0);
        assert_eq!(txn.concept, "Transporte");
        assert_eq!(txn.subconcept.as_deref(), Some("combustible"));
    }

    #[test]
    fn test_parse_row_income() {
        let cells: Vec<String> = vec![
            "$4,742.92", "2/7/2026", "Nomina", "", "", "",
        ].into_iter().map(String::from).collect();
        let txn = parse_row(&cells, 0, 7, 2026).unwrap();
        assert_eq!(txn.amount, 4742.92);
        assert_eq!(txn.concept, "Nomina");
    }

    #[test]
    fn test_parse_row_scan() {
        let cells: Vec<String> = vec![
            "$116.00", "1/7/2026", "Discrecional", "salidas", "Liquido", "cafe",
            "$4,742.92", "2/7/2026", "Nomina", "", "", "",
            "$2,615.00", "3/7/2026", "Voluntario", "", "", "",
        ].into_iter().map(String::from).collect();
        let expense = parse_row(&cells, 0, 7, 2026).unwrap();
        assert_eq!(expense.amount, -116.0);
        assert_eq!(expense.concept, "Discrecional");

        let income = parse_row(&cells, 6, 7, 2026).unwrap();
        assert_eq!(income.amount, 4742.92);
        assert_eq!(income.concept, "Nomina");

        let savings = parse_row(&cells, 12, 7, 2026).unwrap();
        assert_eq!(savings.amount, 2615.0);
        assert_eq!(savings.concept, "Voluntario");
    }
}
