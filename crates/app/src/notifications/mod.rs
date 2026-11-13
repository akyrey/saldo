//! Notification channels and the notifier's outbox (ARCHITECTURE.md 5.5 and
//! 8.4). Sending itself lives in the server (`saldo_server::notify`), which
//! owns the HTTP and SMTP clients.

mod channels;
mod config;
mod message;
mod outbox;

use sqlx::SqlitePool;

use crate::clock::SharedClock;

pub use channels::{Channel, ChannelUpdate, ChannelView, NewChannel};
pub use config::{ChannelConfig, ChannelKind, ChannelSettings};
pub use message::{Message, NOTIFYING_EVENTS};
pub use outbox::{CURSOR, MAX_ATTEMPTS, PendingNotification};

#[derive(Clone)]
pub struct NotificationService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl NotificationService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }
}
