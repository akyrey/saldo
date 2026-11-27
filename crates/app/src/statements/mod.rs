//! Bank and Satispay statement import (Phase 13, Track B).

mod booking;
mod classify;
mod commit;
mod enrich;
mod import_repo;
mod imports;
mod ledger_repo;
mod match_repo;
pub mod parse;
mod preview;
mod profile_repo;
mod profiles;
mod review;
mod source_lookup;
mod summary;
mod views;

use sqlx::{SqliteConnection, SqlitePool};

use crate::accounts::AccountService;
use crate::clock::SharedClock;
use crate::error::AppError;
use crate::transactions::TransactionService;

pub use preview::{PreviewLine, PreviewView, UnsupportedRow};
pub(crate) use profiles::all_in as profiles_all_in;
pub use profiles::{Preset, ProfileInput, ProfileView, TransferRule, presets};
pub use views::{ImportDetail, ImportView, LineUpdate, LineView, PartView, UploadInput};

#[derive(Clone)]
pub struct StatementService {
    pool: SqlitePool,
    clock: SharedClock,
    transactions: TransactionService,
    accounts: AccountService,
}

impl StatementService {
    pub fn new(
        pool: SqlitePool,
        clock: SharedClock,
        transactions: TransactionService,
        accounts: AccountService,
    ) -> Self {
        Self {
            pool,
            clock,
            transactions,
            accounts,
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
