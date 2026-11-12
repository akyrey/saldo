//! Saldo application layer: services (use cases), repositories and migrations.
//!
//! Every state-changing service method opens a database transaction,
//! validates through `saldo-core`, writes the rows, appends one event, and
//! commits. Every method takes a [`ctx::HouseholdCtx`] first.

pub mod clock;
pub mod ctx;
pub mod db;
pub mod error;
pub mod events;
pub mod page;

use sqlx::SqlitePool;

use crate::clock::SharedClock;
use crate::events::EventService;

/// The entry point: one value holding every service. Built once at startup
/// and cloned cheaply (the pool and the clock are shared pointers).
/// New feature modules register their service here, in one place.
#[derive(Clone)]
pub struct App {
    pub pool: SqlitePool,
    pub clock: SharedClock,
    pub events: EventService,
}

impl App {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self {
            events: EventService::new(pool.clone()),
            pool,
            clock,
        }
    }
}

/// Returns the application name from `saldo-core`.
pub fn app_name() -> &'static str {
    saldo_core::APP_NAME
}
