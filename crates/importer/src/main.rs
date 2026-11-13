//! The `saldo-import` binary.

use std::process::ExitCode;
use std::sync::Arc;

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand};
use saldo_app::clock::SystemClock;
use saldo_app::{App, db};
use saldo_importer::engine::{self, Options};
use saldo_importer::firefly::FireflyClient;

const DEFAULT_DATABASE_URL: &str = "sqlite:///data/saldo.db";

/// `clap` builds the parser from these structs, like an Artisan command
/// signature or a `cobra` command in Go.
#[derive(Debug, Parser)]
#[command(
    name = "saldo-import",
    version,
    about = "Import data into Saldo (reads SALDO_DATABASE_URL)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Import from a Firefly III instance. Firefly is only read, never changed.
    Firefly(FireflyArgs),
}

#[derive(Debug, Args)]
struct FireflyArgs {
    /// Firefly III address, for example https://firefly.example.com
    #[arg(long)]
    url: String,
    /// Name of the environment variable holding the personal access token.
    #[arg(long, default_value = "FIREFLY_TOKEN")]
    token_env: String,
    /// Saldo household ID or name.
    #[arg(long)]
    household: String,
    /// Run everything, print the report, then roll back.
    #[arg(long)]
    dry_run: bool,
    /// Comma-separated entities: currencies,accounts,categories,tags,budgets,transactions,attachments
    #[arg(long, value_delimiter = ',')]
    only: Option<Vec<String>>,
    /// Only transactions on or after this date (YYYY-MM-DD).
    #[arg(long)]
    since: Option<NaiveDate>,
    /// Exit 0 even if balances differ from Firefly.
    #[arg(long)]
    allow_mismatch: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let Command::Firefly(args) = Cli::parse().command;
    match import(args).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn import(args: FireflyArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let token = std::env::var(&args.token_env)
        .ok()
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| format!("environment variable {} is not set", args.token_env))?;
    let firefly = FireflyClient::new(&args.url, token.trim().to_string())?;
    let about = firefly.about().await?;
    eprintln!("Firefly III {} (API {})", about.version, about.api_version);

    let url =
        std::env::var("SALDO_DATABASE_URL").unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string());
    let pool = db::connect(&url).await?;
    db::migrate(&pool).await?;
    let data_dir =
        std::env::var("SALDO_DATA_DIR").unwrap_or_else(|_| saldo_app::DEFAULT_DATA_DIR.to_string());
    let app = App::new(pool, Arc::new(SystemClock)).with_data_dir(data_dir.into());
    let ctx = app.identity.household_context(&args.household).await?;

    let options = Options {
        dry_run: args.dry_run,
        only: args.only,
        since: args.since,
    };
    let report = engine::run(app, firefly, ctx, options).await?;
    println!("{}", report.render());
    if report.balance_mismatches() > 0 && !args.allow_mismatch {
        eprintln!("balance check failed (use --allow-mismatch to accept)");
        return Ok(ExitCode::from(2));
    }
    Ok(ExitCode::SUCCESS)
}
