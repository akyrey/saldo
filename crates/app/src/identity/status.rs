//! Whether the first-run setup still has to happen.

use super::IdentityService;
use crate::error::AppError;

impl IdentityService {
    /// True while no user exists, so the web app can show its setup screen.
    pub async fn needs_setup(&self) -> Result<bool, AppError> {
        let users = sqlx::query_scalar!("SELECT count(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        Ok(users == 0)
    }
}
