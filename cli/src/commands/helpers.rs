use chrono::Datelike;
use money_core::error::AppError;
use money_core::Result;

pub fn get_current_month() -> i32 {
    chrono::Local::now().month() as i32
}

pub fn get_current_year() -> i32 {
    chrono::Local::now().year()
}

pub fn get_concept_names(conn: &rusqlite::Connection, type_filter: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM concepts WHERE concept_type IN (?1, 'both') ORDER BY name",
    )?;
    let rows = stmt.query_map(rusqlite::params![type_filter], |row| {
        row.get::<_, String>(0)
    })?;
    let mut names = Vec::new();
    for row in rows {
        names.push(row?);
    }
    Ok(names)
}

pub fn get_bucket_names(conn: &rusqlite::Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM buckets ORDER BY name")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut names = Vec::new();
    for row in rows {
        names.push(row?);
    }
    Ok(names)
}

pub fn get_today() -> String {
    let now = chrono::Local::now();
    format!("{}-{:02}-{:02}", now.year(), now.month(), now.day())
}

/// Convert dialoguer errors to AppError
pub fn map_dlg_err<T>(r: std::result::Result<T, dialoguer::Error>) -> Result<T> {
    r.map_err(|e| AppError::Config(e.to_string()))
}
