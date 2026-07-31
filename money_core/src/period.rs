use chrono::{Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// Canonical "today" as YYYY-MM-DD, single source of truth for the current date.
pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// Validates that `date` is a well-formed, zero-padded YYYY-MM-DD calendar
/// date. chrono's `%Y-%m-%d` alone accepts non-zero-padded input like
/// "2026-7-4", which would silently defeat the half-open date-range indexing
/// every report query relies on, so the exact length/shape is checked too.
pub fn validate_date(date: &str) -> Result<()> {
    let ok = date.len() == 10
        && NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map(|d| d.format("%Y-%m-%d").to_string() == date)
            .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "Invalid date: '{date}' (expected YYYY-MM-DD)"
        )))
    }
}

/// A calendar month, e.g. "2026-07". Backs all period-scoped queries via
/// half-open date ranges (`start()` .. `end_exclusive()`), never `strftime`
/// in a `WHERE` clause, so range queries can use an index on `entries.date`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-rs", ts(export, export_to = "../../gui/src/bindings/"))]
pub struct Period(String);

impl Period {
    pub fn current() -> Period {
        Period(Local::now().format("%Y-%m").to_string())
    }

    pub fn new(year: i32, month: u32) -> Result<Period> {
        if !(1..=12).contains(&month) {
            return Err(AppError::Invalid(format!("Invalid month: {month}")));
        }
        Ok(Period(format!("{year:04}-{month:02}")))
    }

    /// Parses a canonical "YYYY-MM" string.
    pub fn parse(s: &str) -> Result<Period> {
        let d = NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d")
            .map_err(|_| AppError::Invalid(format!("Invalid period: '{s}' (expected YYYY-MM)")))?;
        Ok(Period(format!("{:04}-{:02}", d.year(), d.month())))
    }

    /// Derives the period containing a given YYYY-MM-DD date.
    pub fn from_date(date: &str) -> Result<Period> {
        let d = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| AppError::Invalid(format!("Invalid date: '{date}'")))?;
        Ok(Period(format!("{:04}-{:02}", d.year(), d.month())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Inclusive start date of the period, "YYYY-MM-01".
    pub fn start(&self) -> String {
        format!("{}-01", self.0)
    }

    /// Exclusive end date: the first day of the following month.
    /// Use as `date >= start() AND date < end_exclusive()` so the query
    /// can use an index on `date`.
    pub fn end_exclusive(&self) -> String {
        let d = NaiveDate::parse_from_str(&self.start(), "%Y-%m-%d").expect("valid start date");
        let next = if d.month() == 12 {
            NaiveDate::from_ymd_opt(d.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
        }
        .expect("valid next-month date");
        next.format("%Y-%m-%d").to_string()
    }
}

impl std::fmt::Display for Period {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_period() {
        let p = Period::parse("2026-07").unwrap();
        assert_eq!(p.as_str(), "2026-07");
    }

    #[test]
    fn rejects_malformed_period() {
        assert!(Period::parse("2026-13").is_err());
        assert!(Period::parse("julio").is_err());
    }

    #[test]
    fn from_date_derives_period() {
        let p = Period::from_date("2026-07-15").unwrap();
        assert_eq!(p.as_str(), "2026-07");
    }

    #[test]
    fn half_open_range_crosses_year() {
        let p = Period::new(2026, 12).unwrap();
        assert_eq!(p.start(), "2026-12-01");
        assert_eq!(p.end_exclusive(), "2027-01-01");
    }

    #[test]
    fn half_open_range_mid_year() {
        let p = Period::parse("2026-07").unwrap();
        assert_eq!(p.start(), "2026-07-01");
        assert_eq!(p.end_exclusive(), "2026-08-01");
    }

    #[test]
    fn validate_date_ok_and_err() {
        assert!(validate_date("2026-07-04").is_ok());
        assert!(validate_date("2026-7-4").is_err());
        assert!(validate_date("garbage").is_err());
    }
}
