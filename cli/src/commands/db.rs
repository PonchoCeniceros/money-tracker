use clap::{Args, Subcommand};
use dialoguer::{Input, Password};
use money_core::auth::SupabaseAuth;
use money_core::schema::EXPECTED_SCHEMA_VERSION;
use money_core::services::backup_service;
use money_core::Result;
use money_core::Settings;

use crate::commands::helpers;

#[derive(Args)]
pub struct DbArgs {
    #[command(subcommand)]
    pub command: DbCommands,
}

#[derive(Subcommand)]
pub enum DbCommands {
    /// Back up the whole ledger to a SQL file (restorable in a fresh Supabase project)
    Backup(BackupArgs),
    /// Manage the Supabase connection (login, logout, status)
    Remote(RemoteArgs),
}

#[derive(Args)]
pub struct BackupArgs {
    /// Destination folder or new file (default: ~/.money-tracker/backups/)
    #[arg(short = 'o', long)]
    output: Option<std::path::PathBuf>,
}

#[derive(Args)]
pub struct RemoteArgs {
    #[command(subcommand)]
    pub command: RemoteCommands,
}

#[derive(Subcommand)]
pub enum RemoteCommands {
    /// Sign in and store the session (refresh token) + url/key
    Login(LoginArgs),
    /// Forget the stored session (keeps url/key so only a re-login is needed)
    Logout,
    /// Show connection, session, ledger revision and schema version
    Status,
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

pub fn run(args: DbArgs) -> Result<()> {
    match args.command {
        DbCommands::Backup(b) => backup(b),
        DbCommands::Remote(a) => match a.command {
            RemoteCommands::Login(la) => remote_login(la),
            RemoteCommands::Logout => remote_logout(),
            RemoteCommands::Status => remote_status(),
        },
    }
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

    // Load-modify-save so other keys (token_storage) survive the rewrite.
    let mut settings = Settings::load();
    settings.supabase_url = Some(url);
    settings.supabase_publishable_key = Some(key);
    money_core::settings::save_settings(&settings)?;

    println!("✓ Sesión iniciada como {email}");
    println!("  El refresh token quedó guardado en: {}.", auth.storage().label());
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
    let (url, key) = match (&settings.supabase_url, &settings.supabase_publishable_key) {
        (Some(u), Some(k)) => (u.clone(), k.clone()),
        _ => {
            println!("Conexión:        no configurada");
            println!("                 Corre: money-tracker db remote login --url … --key …");
            print_last_backup();
            return Ok(());
        }
    };
    println!("Conexión:        {url}");

    let auth = SupabaseAuth::new(&url, &key);
    let storage = auth.storage().label();
    if auth.load_refresh_token()?.is_none() {
        println!("Sesión:          no iniciada · se guardaría en {storage}");
        println!("                 Corre: money-tracker db remote login");
        return Ok(());
    }

    match helpers::backend() {
        Ok(be) => {
            let status = be.status()?;
            let email = be.session_email().unwrap_or_else(|| "?".into());
            println!("Sesión:          activa ({email}) · guardada en {storage}");
            println!("Revisión:        {}", status.revision);
            println!(
                "Esquema:         versión {} (la app espera {EXPECTED_SCHEMA_VERSION})",
                status.schema_version
            );
        }
        Err(e) => {
            println!("Sesión:          guardada en {storage}, pero no se pudo conectar");
            println!("                 {e}");
        }
    }
    print_last_backup();
    Ok(())
}

fn print_last_backup() {
    match backup_service::last_backup() {
        Some(r) => println!("Último respaldo: {} · {}", r.at, r.path),
        None => println!("Último respaldo: —"),
    }
}

fn backup(args: BackupArgs) -> Result<()> {
    let be = helpers::backend()?;
    let info = backup_service::create(&*be, args.output.as_deref())?;
    println!("✓ Respaldo: {} ({} movimientos)", info.path, info.entries);
    println!("  Revisión {} · esquema versión {}", info.revision, info.schema_version);
    Ok(())
}
