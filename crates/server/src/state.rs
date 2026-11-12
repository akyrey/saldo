//! State shared by every request.

use std::sync::Arc;

use saldo_app::App;

use crate::config::Config;
use crate::rate_limit::LoginLimiter;

/// Cloned into every handler. `App` is already cheap to clone (a pool and a
/// shared clock); `Arc` shares one `Config` between all threads without
/// copying it. In Go you would share a pointer; in Laravel, a singleton.
#[derive(Clone)]
pub struct AppState {
    pub app: App,
    pub config: Arc<Config>,
    pub login_limiter: Arc<LoginLimiter>,
}

impl AppState {
    pub fn new(app: App, config: Config) -> Self {
        Self {
            app,
            config: Arc::new(config),
            login_limiter: Arc::new(LoginLimiter::default()),
        }
    }
}
