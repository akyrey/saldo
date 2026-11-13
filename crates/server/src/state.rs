//! State shared by every request.

use std::sync::Arc;

use saldo_app::App;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::notify::Senders;
use crate::rate_limit::LoginLimiter;
use crate::workers::EventSignal;

/// Cloned into every handler. `App` is already cheap to clone (a pool and a
/// shared clock); `Arc` shares one `Config` between all threads without
/// copying it. In Go you would share a pointer; in Laravel, a singleton.
#[derive(Clone)]
pub struct AppState {
    pub app: App,
    pub config: Arc<Config>,
    pub login_limiter: Arc<LoginLimiter>,
    /// Bumped after every successful write; wakes the background tasks.
    pub events: EventSignal,
    /// Cancelled when the server starts shutting down, so endless responses
    /// (the event stream) end and graceful shutdown can finish.
    pub shutdown: CancellationToken,
    /// One notification sender per channel kind.
    pub senders: Arc<Senders>,
    /// Logins on their way through the identity provider.
    pub oidc_logins: Arc<crate::oidc::PendingLogins>,
}

impl AppState {
    pub fn new(app: App, config: Config) -> Self {
        Self {
            senders: Arc::new(Senders::from_config(&config)),
            oidc_logins: Arc::default(),
            app,
            config: Arc::new(config),
            login_limiter: Arc::new(LoginLimiter::default()),
            events: EventSignal::new(),
            shutdown: CancellationToken::new(),
        }
    }
}
