use clap::{ArgAction, Args, Subcommand};
use dialoguer::{Confirm, Input, Password};
use money_core::auth::SupabaseAuth;
use money_core::db as core_db;
use money_core::services::account_service;
use money_core::storage::sqlite::SqliteBackend;
use money_core::sync::{production_backend, MirroringBackend};
use money_core::Settings;
use money_core::{LedgerBackend, Result};

use crate::commands::helpers;

#[derive(Args)]
pub struct DbArgs {
    #[command(subcommand)]
    command: DbCommands,
}

#[derive(Subcommand)]
enum DbCommands {
    /// Show the database path, schema version, and record counts
    Status,
    /// Move the current database aside so a fresh one can be created
    Reset(ResetArgs),
    /// Manage the Supabase remote (login, logout, status, migrate, sync)
    Remote(RemoteArgs),
}

#[derive(Args)]
pub struct ResetArgs {
    /// Keep the old file instead of discarding it (recommended)
    #[arg(
        long,
        action = ArgAction::Set,
        num_args = 0..=1,
        default_value_t = true,
        default_missing_value = "true"
    )]
    backup: bool,
    #[arg(long)]
    yes: bool,
}

#[derive(Args)]
pub struct RemoteArgs {
    #[command(subcommand)]
    command: RemoteCommands,
}

#[derive(Subcommand)]
enum RemoteCommands {
    /// Sign in and store the remote session (refresh token) + config
    Login(LoginArgs),
    /// Forget the stored session (keeps url/key so only a re-login is needed)
    Logout,
    /// Show remote mode, session state, and ledger parity
    Status,
    /// Push a local database (the current ledger file) into Supabase
    Migrate(MigrateArgs),
    /// Pull remote changes into the local mirror once
    Sync,
}

#[derive(Args)]
pub struct LoginArgs {
    /// Supabase project URL (also MONEY_TRACKER_SUPABASE_URL)
    #[arg(long)]
    url: Option<String>,
    /// Supabase Publishable (anon) key (also MONEY_TRACKER_SUPABASE_KEY)
    #[arg(long)]
    key: Option<String>,
    email: Option<String>,
}

#[derive(Args)]
pub struct MigrateArgs {
    #[arg(long)]
    yes: bool,
    /// Allow an already-populated remote (data is appended, never merged)
    #[arg(long)]
    force: bool,
}

pub fn run(args: DbArgs) -> Result<()> {
    match args.command {
        DbCommands::Status => status(),
        DbCommands::Reset(a) => reset(a),
        DbCommands::Remote(a) => match a.command {
            RemoteCommands::Login(la) => remote_login(la),
            RemoteCommands::Logout => remote_logout(),
            RemoteCommands::Status => remote_status(),
            RemoteCommands::Migrate(ma) => remote_migrate(ma),
            RemoteCommands::Sync => remote_sync(),
        },
    }
}

fn status() -> Result<()> {
    let be = helpers::backend()?;

    let path = core_db::db_path();
    println!("Ruta: {}", path.display());

    if !path.exists() {
        println!("(no existe todavía — se crea al primer comando)");
        return Ok(());
    }

    let conn = core_db::open_db()?;
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let accounts = account_service::list_accounts(&*be, true)?;

    println!("Esquema: v{version}");
    println!(
        "Cuentas: {} ({} archivadas)",
        accounts.len(),
        accounts.iter().filter(|a| a.archived).count()
    );
    println!("Movimientos: {}", be.entries(&money_core::services::entry_service::EntryFilter::default())?.len());
    Ok(())
}

fn reset(args: ResetArgs) -> Result<()> {
    let path = core_db::db_path();
    if !path.exists() {
        println!("No hay base de datos en {}", path.display());
        return Ok(());
    }

    if !args.yes {
        let confirmed = helpers::map_dlg_err(
            Confirm::new()
                .with_prompt(format!("¿Mover {} a un lado y empezar limpio?", path.display()))
                .default(false)
                .interact(),
        )?;
        if !confirmed {
            println!("Cancelado.");
            return Ok(());
        }
    }

    if args.backup {
        let timestamp = chrono::Local::now().format("%Y%m%d%H%M%S");
        let mut backup_path = path.clone();
        backup_path.set_file_name(format!(
            "{}.backup-{timestamp}",
            path.file_name().unwrap().to_string_lossy()
        ));
        std::fs::rename(&path, &backup_path)?;
        println!("✓ Respaldado en {}", backup_path.display());
    } else {
        std::fs::remove_file(&path)?;
        println!("✓ Base de datos eliminada");
    }
    Ok(())
}

/// Resolves the remote url/key, falling back to persisted settings. Errors
/// with a clear hint when remote mode isn't configured at all.
fn remote_credentials(url: Option<String>, key: Option<String>) -> Result<(String, String)> {
    let settings = Settings::load();
    let url = url.or(settings.supabase_url).ok_or_else(|| {
        money_core::AppError::Config(
            "No hay URL de Supabase. Pásala con --url o configura MONEY_TRACKER_SUPABASE_URL."
                .into(),
        )
    })?;
    let key = key.or(settings.supabase_publishable_key).ok_or_else(|| {
        money_core::AppError::Config(
            "No hay publishable key. Pásala con --key o configura MONEY_TRACKER_SUPABASE_KEY.".into(),
        )
    })?;
    Ok((url, key))
}

fn remote_login(args: LoginArgs) -> Result<()> {
    let (url, key) = remote_credentials(args.url.clone(), args.key.clone())?;
    let auth = SupabaseAuth::new(&url, &key);

    let email = match args.email {
        Some(e) => e,
        None => helpers::map_dlg_err(Input::new().with_prompt("Email").interact_text())?,
    };
    let password = helpers::map_dlg_err(Password::new().with_prompt("Password").interact())?;

    auth.login(&email, &password)?;

    money_core::settings::save_settings(&Settings {
        supabase_url: Some(url),
        supabase_publishable_key: Some(key),
    })?;

    println!("✓ Sesión iniciada como {email}");
    println!("  El refresh token quedó guardado en el llavero.");
    println!("  Verifica la conexión con: money-tracker db remote status");
    Ok(())
}

fn remote_logout() -> Result<()> {
    let settings = Settings::load();
    let (url, key) = match (settings.supabase_url, settings.supabase_publishable_key) {
        (Some(u), Some(k)) => (u, k),
        _ => (String::new(), String::new()),
    };
    SupabaseAuth::new(&url, &key).clear_refresh_token()?;
    println!("✓ Sesión cerrada (refresh token eliminado)");
    println!("  URL y key siguen configurados — solo vuelve a iniciar sesión con `db remote login`");
    Ok(())
}

fn remote_status() -> Result<()> {
    let settings = Settings::load();

    let mode = if settings.is_complete() {
        "remoto (Supabase)"
    } else if settings.remote_configured() {
        "incompleta — falta la publishable key"
    } else {
        "local (sin Supabase)"
    };
    println!("Modo: {mode}");

    if let Some(url) = &settings.supabase_url {
        println!("URL : {url}");
    }
    if let Some(key) = &settings.supabase_publishable_key {
        println!("Key : {}…{}", &key[..6.min(key.len())], &key[key.len().saturating_sub(4)..]);
    }

    let (url, key) = match (&settings.supabase_url, &settings.supabase_publishable_key) {
        (Some(u), Some(k)) => (u.clone(), k.clone()),
        _ => return Ok(()),
    };
    let auth = SupabaseAuth::new(&url, &key);
    match auth.load_refresh_token()? {
        Some(_) => println!("Sesión: activa (refresh token guardado)"),
        None => println!("Sesión: no iniciada — corre `money-tracker db remote login`"),
    }

    let be = match production_backend(&settings) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return Ok(());
        }
    };

    match be.remote_revision() {
        Ok(rev) => println!("Revisión remota: {rev}"),
        Err(e) => println!("Revisión remota: no disponible ({e})"),
    }
    match be.sync_cursor() {
        Ok(cursor) => println!("Espejo local: en revisión {cursor}"),
        Err(e) => println!("Espejo local: no disponible ({e})"),
    }
    if let Some(w) = be.take_sync_warning() {
        eprintln!("Aviso: {w}");
    }
    Ok(())
}

fn remote_migrate(args: MigrateArgs) -> Result<()> {
    let (url, key) = remote_credentials(None, None)?;
    let path = core_db::db_path();
    if !path.exists() {
        println!("No hay base de datos local en {} — nada que migrar.", path.display());
        return Ok(());
    }

    let remote = money_core::storage::remote::SupabaseBackend::new(&url, &key);
    let local = SqliteBackend::open_at(&path)?;

    let existing = remote.list_accounts(false)?;
    if !existing.is_empty() && !args.force {
        eprintln!(
            "El remoto ya tiene {} cuentas. La migración añade (no fusiona); usa --force para continuar.",
            existing.len()
        );
        return Ok(());
    }

    if !args.yes {
        let confirmed = helpers::map_dlg_err(
            Confirm::new()
                .with_prompt("¿Migrar la base local a Supabase remoto?")
                .default(false)
                .interact(),
        )?;
        if !confirmed {
            println!("Cancelado.");
            return Ok(());
        }
    }

    let summary = money_core::sync::migrate_local_to_remote(&local, &remote)?;
    println!("✓ Migrados: {} cuentas · {} movimientos · {} conceptos · {} presupuestos",
             summary.accounts, summary.entries, summary.concepts, summary.budgets);

    let mirror = SqliteBackend::open_at(&money_core::settings::mirror_path())?;
    mirror.reset_mirror_cursor()?;
    let delta = remote.pull_changes_since(0)?;
    mirror.apply_remote_snapshot(&delta)?;
    println!("✓ Espejo local reconstruido desde el remoto (revisión {})", mirror.sync_cursor()?);

    if let Some(w) = mirror.take_sync_warning() {
        eprintln!("Aviso: {w}");
    }
    Ok(())
}

fn remote_sync() -> Result<()> {
    let (url, key) = remote_credentials(None, None)?;

    let remote = Box::new(money_core::storage::remote::SupabaseBackend::new(&url, &key));
    let mirror = Box::new(SqliteBackend::open_at(&money_core::settings::mirror_path())?);
    let sync = MirroringBackend::new(remote, mirror);
    sync.poll()?;

    let be: &dyn LedgerBackend = &sync;
    println!("✓ Sync completado");
    println!("  Revisión remota: {}", be.remote_revision()?);
    println!("  Espejo local:   revisión {}", be.sync_cursor()?);
    if let Some(w) = be.take_sync_warning() {
        eprintln!("Aviso: {w}");
    }
    Ok(())
}