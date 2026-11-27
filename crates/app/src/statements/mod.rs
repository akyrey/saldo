//! Bank and Satispay statement import (Phase 13, Track B).

pub mod parse;
mod profile_repo;
mod profiles;

use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::SharedClock;
use crate::error::AppError;
use crate::transactions::TransactionService;

pub use profiles::{Preset, ProfileInput, ProfileView, TransferRule, presets};

#[derive(Clone)]
pub struct StatementService {
    pool: SqlitePool,
    clock: SharedClock,
    #[allow(dead_code)] // used by commit (next step)
    transactions: TransactionService,
}

impl StatementService {
    pub fn new(pool: SqlitePool, clock: SharedClock, transactions: TransactionService) -> Self {
        Self {
            pool,
            clock,
            transactions,
        }
    }
}

/// The type of one of the household's accounts, `None` when it is not one.
async fn account_type(
    conn: &mut SqliteConnection,
    household_id: &str,
    account: &str,
) -> Result<Option<String>, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT type FROM accounts WHERE household_id = ? AND id = ?",
        household_id,
        account
    )
    .fetch_optional(&mut *conn)
    .await?)
}
