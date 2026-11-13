//! Command-line interface of the `saldo` binary.

use clap::{Parser, Subcommand};

/// `#[derive(Parser)]` generates the argument parser from this struct, like
/// Go struct tags or PHP attributes that generate code at compile time.
#[derive(Debug, Parser)]
#[command(name = "saldo", version, about = "Self-hosted personal finance server")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the HTTP server (configured by SALDO_* environment variables).
    Serve,
    /// Manage users.
    User {
        #[command(subcommand)]
        command: UserCommand,
    },
    /// Print the OpenAPI document as JSON.
    Openapi,
    /// Export a household to a file (owner's view of everything).
    Export {
        /// `json` (one document) or `csv` (a zip of CSV files).
        #[arg(value_enum)]
        format: ExportFormat,
        /// Household ID or name.
        #[arg(long)]
        household: String,
        /// File to write; must not exist yet.
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Copy the whole database to a new file (safe while the server runs).
    Backup {
        /// File to write; must not exist yet.
        path: std::path::PathBuf,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportFormat {
    Json,
    Csv,
}

#[derive(Debug, Subcommand)]
pub enum UserCommand {
    /// Create the first user and household (same as POST /api/v1/setup).
    /// The password is read from standard input.
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        household: String,
        #[arg(long, default_value = "EUR")]
        currency: String,
    },
}
