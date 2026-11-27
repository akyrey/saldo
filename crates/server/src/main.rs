//! The `saldo` binary.

use std::io::BufRead;
use std::process::ExitCode;

use clap::Parser;
use saldo_app::ctx::Origin;
use saldo_app::identity::SetupInput;
use saldo_server::cli::{Cli, Command, ExportFormat, UserCommand};
use saldo_server::config::Config;
use saldo_server::{openapi, server};
use tracing_subscriber::EnvFilter;

/// `#[tokio::main]` starts the async runtime and runs `main` on it.
#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Openapi => print_openapi(),
        Command::Serve => run_server().await,
        Command::Export {
            format,
            household,
            out,
        } => export(format, household, out).await,
        Command::Backup { path } => backup(path).await,
        Command::Restore { archive } => restore(archive).await,
        Command::VapidKeys => {
            print_vapid_keys();
            Ok(())
        }
        Command::User {
            command:
                UserCommand::Create {
                    email,
                    name,
                    household,
                    currency,
                },
        } => create_user(email, name, household, currency).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

type MainResult = Result<(), Box<dyn std::error::Error>>;

fn print_openapi() -> MainResult {
    println!("{}", openapi::document_json()?);
    Ok(())
}

async fn run_server() -> MainResult {
    let config = Config::from_env()?;
    let filter = EnvFilter::try_new(&config.log).map_err(|e| format!("SALDO_LOG: {e}"))?;
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .init();
    server::serve(config).await
}

fn print_vapid_keys() {
    let (private_key, public_key) = saldo_server::notify::config::generate_vapid_keys();
    println!("SALDO_VAPID_PRIVATE_KEY={private_key}");
    println!("# public key (served at /api/v1/push/vapid-key): {public_key}");
}

async fn create_user(
    email: String,
    name: String,
    household: String,
    currency: String,
) -> MainResult {
    let config = Config::from_env()?;
    eprintln!("Password (read from standard input):");
    let mut password = String::new();
    std::io::stdin().lock().read_line(&mut password)?;
    let app = server::open_app(&config).await?;
    let input = SetupInput {
        email,
        password: password.trim_end_matches(['\r', '\n']).to_string(),
        display_name: name,
        household_name: household,
        primary_currency: currency,
    };
    let setup = app.identity.setup(Origin::System, input).await?;
    println!(
        "Created {} in household {} ({})",
        setup.user.email, setup.household.name, setup.household.id
    );
    Ok(())
}

async fn export(format: ExportFormat, household: String, out: std::path::PathBuf) -> MainResult {
    if out.exists() {
        return Err(format!("{} already exists", out.display()).into());
    }
    let config = Config::from_env()?;
    let app = server::open_app(&config).await?;
    let ctx = app.identity.household_context(&household).await?;
    let bytes = match format {
        ExportFormat::Json => serde_json::to_vec_pretty(&app.export.json(&ctx).await?)?,
        ExportFormat::Csv => app.export.csv_zip(&ctx).await?,
    };
    std::fs::write(&out, bytes)?;
    println!("Wrote {}", out.display());
    Ok(())
}

async fn backup(path: std::path::PathBuf) -> MainResult {
    let config = Config::from_env()?;
    let app = server::open_app(&config).await?;
    saldo_app::db::backup(&app.pool, &path).await?;
    println!("Backed up the database to {}", path.display());
    Ok(())
}

async fn restore(archive: std::path::PathBuf) -> MainResult {
    let config = Config::from_env()?;
    let database = server::database_path(&config.database_url)
        .ok_or("SALDO_DATABASE_URL must name a database file to restore into")?;
    let restored = saldo_app::snapshot::restore(&archive, database, &config.data_dir)?;
    let scrubbed = match migrate_and_scrub(&config).await {
        Ok(scrubbed) => scrubbed,
        Err(error) => {
            // An unscrubbed copy must not stay behind, and removing it lets
            // the restore run again.
            remove_database(database);
            return Err(error);
        }
    };
    println!(
        "Restored {} with {} files. Removed {} sessions and tokens, {} webhooks and {} notification channels; log in again.",
        database.display(),
        restored.files,
        scrubbed.sessions,
        scrubbed.webhooks,
        scrubbed.notification_channels
    );
    Ok(())
}

async fn migrate_and_scrub(
    config: &Config,
) -> Result<saldo_app::snapshot::Scrubbed, Box<dyn std::error::Error>> {
    let app = server::open_app(config).await?;
    let scrubbed = saldo_app::snapshot::scrub(&app.pool).await;
    app.pool.close().await;
    Ok(scrubbed?)
}

/// The database and SQLite's `-wal` and `-shm` files beside it.
fn remove_database(database: &std::path::Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut path = database.as_os_str().to_owned();
        path.push(suffix);
        if let Err(error) = std::fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "cannot remove {}: {error}",
                std::path::Path::new(&path).display()
            );
        }
    }
}
