//! Backups: the whole ledger as a SQL file that restores into a fresh Supabase
//! project (after the `setup/sql/` schema files), plus the lazy 7-day
//! automatic backup. Contract: specs/002-remove-mirror-backup/contracts/backup-contract.md.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::models::{BackupInfo, LedgerSnapshot};
use crate::storage::LedgerBackend;

/// `~/.money-tracker/last-backup.toml`: when and where the last successful backup went.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastBackupRecord {
    /// RFC 3339, local time with offset.
    pub at: String,
    pub path: String,
    pub revision: i64,
    pub schema_version: i64,
}

const AUTO_BACKUP_EVERY_DAYS: i64 = 7;

fn sql_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn sql_opt_str(s: &Option<String>) -> String {
    s.as_deref().map_or_else(|| "null".into(), sql_str)
}

fn sql_opt_num<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "null".into(), |n| n.to_string())
}

const USER: &str = "(select id from _restore_user)";

/// The restore script for `s`. `email` prefills the restoring user; `created_at`
/// goes in the header. Output is deterministic (rows by id) so two backups can be
/// compared with `diff`.
pub fn render_sql(s: &LedgerSnapshot, email: &str, created_at: &str) -> String {
    let v = s.schema_version;
    let mut out = format!(
        "-- money-tracker · respaldo del libro contable\n\
         -- creado:          {created_at}\n\
         -- revisión:        {rev}\n\
         -- versión esquema: {v}\n\
         -- usuario origen:  {email}\n\
         -- movimientos:     {n}\n\
         --\n\
         -- Restaurar (proyecto de Supabase nuevo y vacío):\n\
         --   1. Aplica en el SQL Editor, en orden, setup/sql/0001_setup.sql hasta {v:04}_*.sql.\n\
         --   2. Crea tu usuario en Authentication → Users (puede ser el mismo email).\n\
         --   3. Si el email es otro, cámbialo en la línea marcada con «RESTAURAR COMO».\n\
         --   4. Pega este archivo completo en el SQL Editor y dale Run.\n\
         -- Detalle: setup/README.md\n\
         \n\
         begin;\n\
         \n\
         do $$\n\
         begin\n  \
           if (select version from public.schema_version where id = 1) is distinct from {v} then\n    \
             raise exception 'Este respaldo es de la versión de esquema {v} y el proyecto está en %',\n      \
               (select version from public.schema_version where id = 1);\n  \
           end if;\n  \
           if exists (select 1 from public.accounts) or exists (select 1 from public.entries)\n     \
              or exists (select 1 from public.concepts) or exists (select 1 from public.budgets)\n     \
              or exists (select 1 from public.config) then\n    \
             raise exception 'El proyecto no está vacío: solo se restaura sobre un proyecto nuevo';\n  \
           end if;\n\
         end $$;\n\
         \n\
         create temp table _restore_user on commit drop as\n  \
           select id from auth.users where lower(email) = lower({email_sql});  -- RESTAURAR COMO\n\
         do $$\n\
         begin\n  \
           if (select count(*) from _restore_user) <> 1 then\n    \
             raise exception 'No existe exactamente un usuario con ese email en auth.users (revisa «RESTAURAR COMO»)';\n  \
           end if;\n\
         end $$;\n",
        rev = s.revision,
        n = s.entries.len(),
        email_sql = sql_str(email),
    );

    let mut concepts = s.concepts.clone();
    concepts.sort_by_key(|c| c.id);
    push_insert(
        &mut out,
        "public.concepts (id, user_id, name, concept_type)",
        concepts.iter().map(|c| {
            format!("{}, {USER}, {}, {}", sql_opt_num(c.id).replace("null", "default"),
                sql_str(&c.name), sql_str(&c.concept_type))
        }),
    );

    let mut accounts = s.accounts.clone();
    accounts.sort_by_key(|a| a.id);
    push_insert(
        &mut out,
        "public.accounts (id, user_id, name, kind, target_amount, credit_limit, liquid, archived)",
        accounts.iter().map(|a| {
            format!(
                "{}, {USER}, {}, {}, {}, {}, {}, {}",
                a.id, sql_str(&a.name), sql_str(a.kind.as_str()), sql_opt_num(a.target_amount),
                sql_opt_num(a.credit_limit), a.liquid, a.archived
            )
        }),
    );

    let mut entries = s.entries.clone();
    entries.sort_by_key(|e| e.id);
    push_insert(
        &mut out,
        "public.entries (id, user_id, date, kind, amount, from_account_id, to_account_id, concept, subconcept, description)",
        entries.iter().map(|e| {
            format!(
                "{}, {USER}, {}, {}, {}, {}, {}, {}, {}, {}",
                e.id, sql_str(&e.date), sql_str(e.kind.as_str()), e.amount,
                sql_opt_num(e.from_account_id), sql_opt_num(e.to_account_id),
                sql_opt_str(&e.concept), sql_opt_str(&e.subconcept), sql_opt_str(&e.description)
            )
        }),
    );

    let mut budgets = s.budgets.clone();
    budgets.sort_by_key(|b| b.id);
    push_insert(
        &mut out,
        "public.budgets (id, user_id, concept, monthly_limit, period)",
        budgets.iter().map(|b| {
            format!("{}, {USER}, {}, {}, {}", sql_opt_num(b.id).replace("null", "default"),
                sql_str(&b.concept), b.monthly_limit, sql_str(&b.period))
        }),
    );

    let mut config = s.config.clone();
    config.sort_by(|a, b| a.key.cmp(&b.key));
    push_insert(
        &mut out,
        "public.config (user_id, key, value)",
        config.iter().map(|c| format!("{USER}, {}, {}", sql_str(&c.key), sql_str(&c.value))),
    );

    out.push('\n');
    for table in ["concepts", "accounts", "entries", "budgets"] {
        out.push_str(&format!(
            "select setval(pg_get_serial_sequence('public.{table}', 'id'), coalesce(max(id), 1)) from public.{table};\n"
        ));
    }
    out.push_str("\ncommit;\n");
    out
}

/// One multi-row `insert`; nothing at all for an empty table.
fn push_insert(out: &mut String, target: &str, rows: impl Iterator<Item = String>) {
    let rows: Vec<String> = rows.map(|r| format!("  ({r})")).collect();
    if rows.is_empty() {
        return;
    }
    out.push_str(&format!("\ninsert into {target} values\n{};\n", rows.join(",\n")));
}

/// A backup is due when there is no (readable) record or the last one is more
/// than 7 days old.
pub fn is_due(now: DateTime<Local>, record: Option<&LastBackupRecord>) -> bool {
    let last = match record.and_then(|r| DateTime::parse_from_rfc3339(&r.at).ok()) {
        Some(t) => t,
        None => return true,
    };
    now.signed_duration_since(last) > chrono::Duration::days(AUTO_BACKUP_EVERY_DAYS)
}

pub fn read_record_at(path: &Path) -> Option<LastBackupRecord> {
    toml::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

pub fn write_record_at(path: &Path, record: &LastBackupRecord) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(record)
        .map_err(|e| crate::error::AppError::Config(format!("last-backup.toml: {e}")))?;
    std::fs::write(path, text)?;
    Ok(())
}

/// Creates the backup file under `dest` (a directory, or a file path that must
/// not exist yet) and returns its path. Never overwrites (`create_new`), always
/// 0600, and removes the file if writing it fails.
pub fn write_backup_file(dest: &Path, now: DateTime<Local>, content: &str) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let open_new = |path: &Path| {
        std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)
    };

    let (path, mut file) = if dest.is_dir() || dest.extension().is_none() {
        std::fs::create_dir_all(dest)?;
        let stem = format!("money-tracker-{}", now.format("%Y%m%d-%H%M%S"));
        let mut n = 1;
        loop {
            let name = if n == 1 { format!("{stem}.sql") } else { format!("{stem}-{n}.sql") };
            let candidate = dest.join(name);
            match open_new(&candidate) {
                Ok(f) => break (candidate, f),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
                Err(e) => return Err(e.into()),
            }
        }
    } else {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match open_new(dest) {
            Ok(f) => (dest.to_path_buf(), f),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(crate::error::AppError::Invalid(format!(
                    "{} ya existe; un respaldo nunca sobrescribe otro archivo",
                    dest.display()
                )))
            }
            Err(e) => return Err(e.into()),
        }
    };

    if let Err(e) = file.write_all(content.as_bytes()).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(e.into());
    }
    Ok(path)
}

/// Backup of `be` into `dest`, recording it in `record_path`.
pub fn create_at(
    be: &dyn LedgerBackend,
    dest: &Path,
    record_path: &Path,
    now: DateTime<Local>,
    automatic: bool,
) -> Result<BackupInfo> {
    let snapshot = be.export_snapshot()?;
    let created_at = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    let email = be.session_email().unwrap_or_else(|| "CAMBIA-ESTE@EMAIL".into());
    let sql = render_sql(&snapshot, &email, &created_at);
    let path = write_backup_file(dest, now, &sql)?;
    let path_str = path.display().to_string();
    write_record_at(
        record_path,
        &LastBackupRecord {
            at: created_at.clone(),
            path: path_str.clone(),
            revision: snapshot.revision,
            schema_version: snapshot.schema_version,
        },
    )?;
    Ok(BackupInfo {
        path: path_str,
        created_at,
        revision: snapshot.revision,
        schema_version: snapshot.schema_version,
        entries: snapshot.entries.len() as i64,
        automatic,
    })
}

/// Manual backup (`db backup`, GUI button): into `dest` or the default backups folder.
pub fn create(be: &dyn LedgerBackend, dest: Option<&Path>) -> Result<BackupInfo> {
    let default_dir = crate::settings::backups_dir();
    create_at(be, dest.unwrap_or(&default_dir), &crate::settings::last_backup_path(), Local::now(), false)
}

pub fn last_backup() -> Option<LastBackupRecord> {
    read_record_at(&crate::settings::last_backup_path())
}

/// The automatic backup for a caller that already holds a backend (the GUI):
/// `None` if not due.
pub fn run_auto_with(be: &dyn LedgerBackend) -> Option<Result<BackupInfo>> {
    let now = Local::now();
    let record_path = crate::settings::last_backup_path();
    if !is_due(now, read_record_at(&record_path).as_ref()) {
        return None;
    }
    Some(create_at(be, &crate::settings::backups_dir(), &record_path, now, true))
}

/// The lazy automatic backup: only if due, and only then does it `connect`.
/// `None` = not due (nothing happened); `Some(Err)` = due but it failed, so the
/// record is untouched and it retries next time.
pub fn run_auto_if_due(
    now: DateTime<Local>,
    connect: impl FnOnce() -> Result<Box<dyn LedgerBackend>>,
) -> Option<Result<BackupInfo>> {
    let record_path = crate::settings::last_backup_path();
    if !is_due(now, read_record_at(&record_path).as_ref()) {
        return None;
    }
    Some(connect().and_then(|be| {
        create_at(&*be, &crate::settings::backups_dir(), &record_path, now, true)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Account, AccountKind, Concept, Config, Entry, EntryKind};
    use crate::storage::memory::MemoryBackend;
    use chrono::TimeZone;

    fn at(s: &str) -> DateTime<Local> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Local)
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mt-backup-test-{name}-{}-{}",
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(id: i64, date: &str, kind: EntryKind, amount: f64, from: Option<i64>, to: Option<i64>) -> Entry {
        Entry {
            id,
            date: date.into(),
            kind,
            amount,
            from_account_id: from,
            to_account_id: to,
            from_account: None,
            to_account: None,
            concept: None,
            subconcept: None,
            description: None,
        }
    }

    fn snapshot() -> LedgerSnapshot {
        let mut expense = entry(3, "2026-08-10", EntryKind::Expense, 1902.38, Some(3), None);
        expense.concept = Some("Comida de O'Brien".into());
        expense.description = Some("cena 'especial'".into());
        LedgerSnapshot {
            revision: 42,
            schema_version: 2,
            exported_at: "2026-09-26T21:04:11-06:00".into(),
            concepts: vec![
                Concept { id: Some(1), name: "Nomina".into(), concept_type: "income".into() },
                Concept { id: Some(2), name: "Comida de O'Brien".into(), concept_type: "expense".into() },
            ],
            accounts: vec![
                Account {
                    id: 1, name: "debito".into(), kind: AccountKind::Spending,
                    target_amount: None, credit_limit: None, liquid: true, archived: false,
                },
                Account {
                    id: 2, name: "vales".into(), kind: AccountKind::Spending,
                    target_amount: None, credit_limit: None, liquid: false, archived: true,
                },
                Account {
                    id: 3, name: "tdc".into(), kind: AccountKind::Credit,
                    target_amount: None, credit_limit: Some(3000.0), liquid: true, archived: false,
                },
            ],
            entries: vec![
                entry(1, "2026-08-01", EntryKind::Opening, 5000.0, None, Some(1)),
                expense,
                entry(4, "2026-09-05", EntryKind::Transfer, 0.1, Some(1), Some(3)),
            ],
            budgets: vec![],
            config: vec![Config { key: "emergency_pct".into(), value: "10".into() }],
        }
    }

    // --- T036: render_sql -------------------------------------------------------

    #[test]
    fn render_sql_is_a_complete_guarded_restore_script() {
        let sql = render_sql(&snapshot(), "yo@example.com", "2026-09-26T21:04:11-06:00");
        let expected = r#"-- money-tracker · respaldo del libro contable
-- creado:          2026-09-26T21:04:11-06:00
-- revisión:        42
-- versión esquema: 2
-- usuario origen:  yo@example.com
-- movimientos:     3
--
-- Restaurar (proyecto de Supabase nuevo y vacío):
--   1. Aplica en el SQL Editor, en orden, setup/sql/0001_setup.sql hasta 0002_*.sql.
--   2. Crea tu usuario en Authentication → Users (puede ser el mismo email).
--   3. Si el email es otro, cámbialo en la línea marcada con «RESTAURAR COMO».
--   4. Pega este archivo completo en el SQL Editor y dale Run.
-- Detalle: setup/README.md

begin;

do $$
begin
  if (select version from public.schema_version where id = 1) is distinct from 2 then
    raise exception 'Este respaldo es de la versión de esquema 2 y el proyecto está en %',
      (select version from public.schema_version where id = 1);
  end if;
  if exists (select 1 from public.accounts) or exists (select 1 from public.entries)
     or exists (select 1 from public.concepts) or exists (select 1 from public.budgets)
     or exists (select 1 from public.config) then
    raise exception 'El proyecto no está vacío: solo se restaura sobre un proyecto nuevo';
  end if;
end $$;

create temp table _restore_user on commit drop as
  select id from auth.users where lower(email) = lower('yo@example.com');  -- RESTAURAR COMO
do $$
begin
  if (select count(*) from _restore_user) <> 1 then
    raise exception 'No existe exactamente un usuario con ese email en auth.users (revisa «RESTAURAR COMO»)';
  end if;
end $$;

insert into public.concepts (id, user_id, name, concept_type) values
  (1, (select id from _restore_user), 'Nomina', 'income'),
  (2, (select id from _restore_user), 'Comida de O''Brien', 'expense');

insert into public.accounts (id, user_id, name, kind, target_amount, credit_limit, liquid, archived) values
  (1, (select id from _restore_user), 'debito', 'spending', null, null, true, false),
  (2, (select id from _restore_user), 'vales', 'spending', null, null, false, true),
  (3, (select id from _restore_user), 'tdc', 'credit', null, 3000, true, false);

insert into public.entries (id, user_id, date, kind, amount, from_account_id, to_account_id, concept, subconcept, description) values
  (1, (select id from _restore_user), '2026-08-01', 'opening', 5000, null, 1, null, null, null),
  (3, (select id from _restore_user), '2026-08-10', 'expense', 1902.38, 3, null, 'Comida de O''Brien', null, 'cena ''especial'''),
  (4, (select id from _restore_user), '2026-09-05', 'transfer', 0.1, 1, 3, null, null, null);

insert into public.config (user_id, key, value) values
  ((select id from _restore_user), 'emergency_pct', '10');

select setval(pg_get_serial_sequence('public.concepts', 'id'), coalesce(max(id), 1)) from public.concepts;
select setval(pg_get_serial_sequence('public.accounts', 'id'), coalesce(max(id), 1)) from public.accounts;
select setval(pg_get_serial_sequence('public.entries', 'id'), coalesce(max(id), 1)) from public.entries;
select setval(pg_get_serial_sequence('public.budgets', 'id'), coalesce(max(id), 1)) from public.budgets;

commit;
"#;
        assert_eq!(sql, expected);
    }

    #[test]
    fn render_sql_sorts_rows_by_id_and_escapes_the_email() {
        let mut s = snapshot();
        s.accounts.reverse();
        let sql = render_sql(&s, "o'neil@example.com", "x");
        let debito = sql.find("'debito'").unwrap();
        let tdc = sql.find("'tdc'").unwrap();
        assert!(debito < tdc);
        assert!(sql.contains("lower('o''neil@example.com')"));
    }

    // --- T037: is_due + record ----------------------------------------------------

    fn record(at: &str) -> LastBackupRecord {
        LastBackupRecord { at: at.into(), path: "/x.sql".into(), revision: 1, schema_version: 2 }
    }

    #[test]
    fn due_after_more_than_seven_days_or_without_a_record() {
        let now = at("2026-09-26T12:00:00-06:00");
        assert!(is_due(now, None));
        assert!(!is_due(now, Some(&record("2026-09-19T12:00:00-06:00"))));
        assert!(is_due(now, Some(&record("2026-09-19T11:59:59-06:00"))));
        assert!(!is_due(now, Some(&record("2026-09-26T08:00:00-06:00"))));
        // An unparseable date counts as "never backed up".
        assert!(is_due(now, Some(&record("ayer"))));
    }

    #[test]
    fn record_roundtrips_and_a_broken_file_reads_as_none() {
        let dir = scratch("record");
        let path = dir.join("last-backup.toml");
        assert_eq!(read_record_at(&path), None);
        let r = record("2026-09-26T21:04:11-06:00");
        write_record_at(&path, &r).unwrap();
        assert_eq!(read_record_at(&path), Some(r));
        std::fs::write(&path, "esto no es toml [").unwrap();
        assert_eq!(read_record_at(&path), None);
    }

    // --- T038: writing ---------------------------------------------------------------

    #[test]
    fn never_overwrites_and_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("write");
        let now = Local.with_ymd_and_hms(2026, 9, 26, 21, 4, 11).unwrap();
        let first = write_backup_file(&dir, now, "uno").unwrap();
        let second = write_backup_file(&dir, now, "dos").unwrap();
        assert_eq!(first.file_name().unwrap(), "money-tracker-20260926-210411.sql");
        assert_eq!(second.file_name().unwrap(), "money-tracker-20260926-210411-2.sql");
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "uno");
        let mode = std::fs::metadata(&first).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        // An explicit file that already exists is an error, not an overwrite.
        assert!(write_backup_file(&first, now, "tres").is_err());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "uno");
        // An explicit new file path is used as-is.
        let explicit = dir.join("mio.sql");
        assert_eq!(write_backup_file(&explicit, now, "cuatro").unwrap(), explicit);
    }

    #[test]
    fn a_failed_write_leaves_no_file() {
        let dir = scratch("fail");
        let blocked = dir.join("no-existe").join("tampoco").join("x.sql");
        // Parent can't be created: a regular file sits where a directory should be.
        std::fs::write(dir.join("no-existe"), "archivo").unwrap();
        let now = Local.with_ymd_and_hms(2026, 9, 26, 21, 4, 11).unwrap();
        assert!(write_backup_file(&blocked, now, "x").is_err());
        assert!(!blocked.exists());
    }

    #[test]
    fn create_writes_the_file_and_the_record() {
        let be = MemoryBackend::seeded();
        let dir = scratch("create");
        let record_path = dir.join("last-backup.toml");
        let now = Local.with_ymd_and_hms(2026, 9, 26, 21, 4, 11).unwrap();
        let info = create_at(&be, &dir.join("backups"), &record_path, now, false).unwrap();
        assert!(std::fs::read_to_string(&info.path).unwrap().contains("-- usuario origen:  test@example.com"));
        assert!(!info.automatic);
        let rec = read_record_at(&record_path).unwrap();
        assert_eq!(rec.path, info.path);
        assert_eq!(rec.at, info.created_at);
        assert!(!is_due(now, Some(&rec)));
    }
}
