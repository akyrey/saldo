//! Snapshots: the whole instance (database and attachment files) as one zip,
//! to restore into a fresh Saldo for tests (ARCHITECTURE.md 9.3).
//!
//! Unlike the export, a snapshot is the SQLite file itself: every table,
//! the event log and the secrets included. So the download is off unless
//! the server enables it, and only for an instance with a single household;
//! a restore always runs [`scrub`] so the copy cannot reach real webhooks,
//! notification channels or sessions.

mod archive;

use std::fs::File;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::ctx::HouseholdCtx;
use crate::error::AppError;

pub use archive::{DATABASE_ENTRY, Restored};

#[derive(Clone)]
pub struct SnapshotService {
    pool: SqlitePool,
    data_dir: PathBuf,
}

impl SnapshotService {
    pub fn new(pool: SqlitePool, data_dir: PathBuf) -> Self {
        Self { pool, data_dir }
    }

    /// Writes a snapshot and returns it as an open file whose name is
    /// already removed, so it disappears once the caller drops it (Unix).
    /// Owner only, and only while this household is the instance's only one.
    pub async fn create(&self, ctx: &HouseholdCtx) -> Result<File, AppError> {
        ctx.require_owner()?;
        self.require_single_household(ctx).await?;
        tokio::fs::create_dir_all(&self.data_dir)
            .await
            .map_err(|e| AppError::Internal(format!("snapshot: {e}")))?;
        let id = Uuid::now_v7();
        let database = self.data_dir.join(format!(".snapshot-{id}.db"));
        let zip = self.data_dir.join(format!(".snapshot-{id}.zip"));
        // Removes both temporary files however this function ends: an error,
        // or the request being cancelled while it waits (the future dropped).
        let _cleanup = RemoveOnDrop(vec![database.clone(), zip.clone()]);
        crate::db::backup(&self.pool, &database).await?;
        let data_dir = self.data_dir.clone();
        // `spawn_blocking` moves blocking file work to a thread pool, so it
        // does not stall the async runtime (Go would just run a goroutine).
        tokio::task::spawn_blocking(move || write_unlinked(&database, &data_dir, &zip))
            .await
            .map_err(|e| AppError::Internal(format!("snapshot task: {e}")))?
    }

    async fn require_single_household(&self, ctx: &HouseholdCtx) -> Result<(), AppError> {
        let household_id = ctx.household_id.to_string();
        let others = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM households WHERE id <> ?",
            household_id
        )
        .fetch_one(&self.pool)
        .await?;
        if others > 0 {
            return Err(AppError::Conflict(
                "a snapshot holds the whole instance; it is only offered while this is the only household"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

/// Writes the zip, then removes its name while keeping the file open. The
/// zip's name is removed on failure too, even if the caller is gone by then.
fn write_unlinked(database: &Path, data_dir: &Path, zip: &Path) -> Result<File, AppError> {
    let _cleanup = RemoveOnDrop(vec![zip.to_path_buf()]);
    let io = |e: std::io::Error| AppError::Internal(format!("snapshot: {e}"));
    let out = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(zip)
        .map_err(io)?;
    let mut file = archive::write(database, data_dir, out)?;
    std::io::Seek::rewind(&mut file).map_err(io)?;
    Ok(file)
}

/// Removes its paths when dropped, like Go's `defer os.Remove(path)`. `Drop`
/// runs on every way out of a scope, including a cancelled async function.
/// A failure only leaves a hidden file behind, so it is logged, not returned.
struct RemoveOnDrop(Vec<PathBuf>);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        for path in &self.0 {
            if let Err(error) = std::fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(path = %path.display(), %error, "cannot remove snapshot temporary file");
            }
        }
    }
}

/// Unpacks a snapshot into a fresh instance (`saldo restore`); see
/// [`archive::restore`]. Run the migrations and [`scrub`] afterwards.
pub fn restore(archive: &Path, database: &Path, data_dir: &Path) -> Result<Restored, AppError> {
    archive::restore(archive, database, data_dir)
}

/// What [`scrub`] removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Scrubbed {
    pub sessions: u64,
    pub webhooks: u64,
    pub notification_channels: u64,
}

/// Removes what lets a restored copy act as the original: sessions and API
/// tokens, webhook endpoints (with their secrets and deliveries) and
/// notification channels (with their credentials and queued sends).
///
/// It runs on the whole instance, takes no `HouseholdCtx` and appends no
/// events: it is maintenance on a copy before its first start, like the
/// attachment sweep, not a household's own change.
pub async fn scrub(pool: &SqlitePool) -> Result<Scrubbed, AppError> {
    let mut tx = crate::db::begin(pool).await?;
    sqlx::query!("DELETE FROM notifications")
        .execute(&mut *tx)
        .await?;
    let notification_channels = sqlx::query!("DELETE FROM notification_channels")
        .execute(&mut *tx)
        .await?
        .rows_affected();
    sqlx::query!("DELETE FROM webhook_deliveries")
        .execute(&mut *tx)
        .await?;
    let webhooks = sqlx::query!("DELETE FROM webhook_endpoints")
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let sessions = sqlx::query!("DELETE FROM sessions")
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(Scrubbed {
        sessions,
        webhooks,
        notification_channels,
    })
}
