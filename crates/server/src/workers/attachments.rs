//! The attachment sweep: removes stored files that no attachment uses any
//! more (left by deleted transactions, splits and bills, or by an upload
//! whose database transaction rolled back), at startup and hourly.

use std::time::Duration;

use saldo_app::App;
use tracing::info;

pub const NAME: &str = "attachment-sweep";
pub const INTERVAL: Duration = Duration::from_secs(60 * 60);
/// Younger files may belong to an upload whose row is not committed yet.
const MIN_AGE: Duration = Duration::from_secs(60 * 60);

pub async fn run(app: App) -> Result<(), String> {
    let removed = app
        .attachments
        .remove_unused_files(MIN_AGE)
        .await
        .map_err(|e| e.to_string())?;
    if removed > 0 {
        info!(removed, "removed unused attachment files");
    }
    Ok(())
}
