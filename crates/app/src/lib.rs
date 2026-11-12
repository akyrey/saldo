//! Saldo application layer: services (use cases), repositories and migrations.
//!
//! Every state-changing service method opens a database transaction,
//! validates through `saldo-core`, writes the rows, appends one event, and
//! commits. Every method takes a [`ctx::HouseholdCtx`] first.

pub mod accounts;
pub mod categories;
pub mod clock;
pub mod ctx;
pub mod currencies;
pub mod db;
pub mod error;
pub mod events;
pub mod identity;
pub mod page;
pub mod scope;
pub mod tags;
pub mod transactions;

use sqlx::SqlitePool;

use crate::accounts::AccountService;
use crate::categories::CategoryService;
use crate::clock::SharedClock;
use crate::currencies::CurrencyService;
use crate::events::EventService;
use crate::identity::IdentityService;
use crate::tags::TagService;
use crate::transactions::TransactionService;

/// The entry point: one value holding every service. Built once at startup
/// and cloned cheaply (the pool and the clock are shared pointers).
/// New feature modules register their service here, in one place.
#[derive(Clone)]
pub struct App {
    pub pool: SqlitePool,
    pub clock: SharedClock,
    pub accounts: AccountService,
    pub categories: CategoryService,
    pub currencies: CurrencyService,
    pub events: EventService,
    pub identity: IdentityService,
    pub tags: TagService,
    pub transactions: TransactionService,
}

impl App {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self {
            accounts: AccountService::new(pool.clone(), clock.clone()),
            categories: CategoryService::new(pool.clone(), clock.clone()),
            currencies: CurrencyService::new(pool.clone()),
            events: EventService::new(pool.clone()),
            identity: IdentityService::new(pool.clone(), clock.clone()),
            tags: TagService::new(pool.clone(), clock.clone()),
            transactions: TransactionService::new(pool.clone(), clock.clone()),
            pool,
            clock,
        }
    }
}

impl App {
    /// Checks the database answers (for `/healthz`).
    pub async fn ping(&self) -> Result<(), error::AppError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

/// Returns the application name from `saldo-core`.
pub fn app_name() -> &'static str {
    saldo_core::APP_NAME
}
