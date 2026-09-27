//! The Supabase schema version this build expects.
//!
//! Schema changes ship as numbered files in `supabase/sql/` (`NNNN_name.sql`), applied by
//! hand in the Supabase SQL Editor. Each file checks it follows the previous version and
//! bumps `public.schema_version`; the app refuses to run against any other version.

/// Must equal the highest-numbered file in `supabase/sql/` (enforced by a test below).
pub const EXPECTED_SCHEMA_VERSION: i64 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_version_matches_the_latest_schema_file() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../supabase/sql");
        let mut numbers: Vec<i64> = std::fs::read_dir(dir)
            .expect("supabase/sql exists")
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".sql"))
            .map(|name| {
                let (num, rest) = name.split_once('_').unwrap_or_else(|| panic!("bad name: {name}"));
                assert!(
                    num.len() == 4 && num.chars().all(|c| c.is_ascii_digit()),
                    "{name}: must start with 4 digits"
                );
                let stem = rest.strip_suffix(".sql").unwrap();
                assert!(
                    !stem.is_empty()
                        && stem.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                    "{name}: name must be lowercase letters, digits and _"
                );
                num.parse().unwrap()
            })
            .collect();
        numbers.sort();
        let expected: Vec<i64> = (1..=numbers.len() as i64).collect();
        assert_eq!(numbers, expected, "schema files must be numbered 0001, 0002, … with no gaps");
        assert_eq!(
            numbers.last().copied(),
            Some(EXPECTED_SCHEMA_VERSION),
            "bump EXPECTED_SCHEMA_VERSION when adding supabase/sql/NNNN_*.sql"
        );
    }
}
