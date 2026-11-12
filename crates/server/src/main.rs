//! The `saldo` binary.

use std::io::BufRead;
use std::process::ExitCode;

use clap::Parser;
use saldo_app::ctx::Origin;
use saldo_app::identity::SetupInput;
use saldo_server::cli::{Cli, Command, UserCommand};
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
