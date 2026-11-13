//! Attachments: files linked to a split, an account, a tag or a bill
//! (budgets and piggy banks can join later).

mod storage;

use std::path::PathBuf;

use saldo_core::AttachmentId;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};

/// Largest accepted file: 20 MiB.
pub const MAX_SIZE: usize = 20 * 1024 * 1024;

/// MIME types accepted for upload. Downloads are always sent as
/// `attachment` with `nosniff`, so a browser never renders them inline.
pub const ALLOWED_MIME_TYPES: [&str; 11] = [
    "application/pdf",
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/heic",
    "text/plain",
    "text/csv",
    "application/xml",
    "text/xml",
    "application/octet-stream",
];

/// What attachments can be linked to (budgets and piggy banks join later).
pub const ATTACHABLE_TYPES: [&str; 4] = ["split", "account", "tag", "bill"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct AttachmentView {
    pub id: String,
    /// `split`, `account`, `tag` or `bill`.
    pub attachable_type: String,
    pub attachable_id: String,
    pub filename: String,
    pub title: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub notes: Option<String>,
    pub created_at: String,
}

/// A file to attach.
#[derive(Debug, Clone)]
pub struct NewAttachment {
    pub attachable_type: String,
    pub attachable_id: String,
    pub filename: String,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct AttachmentService {
    pool: SqlitePool,
    clock: SharedClock,
    data_dir: PathBuf,
}

impl AttachmentService {
    pub fn new(pool: SqlitePool, clock: SharedClock, data_dir: PathBuf) -> Self {
        Self {
            pool,
            clock,
            data_dir,
        }
    }

    pub async fn upload(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewAttachment,
    ) -> Result<AttachmentView, AppError> {
        let mut tx = self.pool.begin().await?;
        let view = self.upload_in(&mut tx, ctx, origin, input).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Like [`Self::upload`], inside the caller's transaction. The file is
    /// written first; if the transaction rolls back, the file stays as an
    /// unreferenced copy, which is harmless.
    pub async fn upload_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewAttachment,
    ) -> Result<AttachmentView, AppError> {
        check(&input)?;
        check_target(conn, ctx, &input.attachable_type, &input.attachable_id).await?;
        let sha256 = storage::sha256_hex(&input.bytes);
        storage::store(&self.data_dir, &sha256, &input.bytes).await?;

        let id = new_id(&*self.clock).to_string();
        let (household_id, now) = (
            ctx.household_id.to_string(),
            format_timestamp(self.clock.now()),
        );
        let size =
            i64::try_from(input.bytes.len()).map_err(|_| AppError::invalid("file", "too large"))?;
        let filename = input.filename.trim().to_string();
        sqlx::query!(
            "INSERT INTO attachments (id, household_id, attachable_type, attachable_id, filename, title,
                mime_type, size_bytes, sha256, notes, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            id, household_id, input.attachable_type, input.attachable_id, filename, input.title,
            input.mime_type, size, sha256, input.notes, now,
        )
        .execute(&mut *conn)
        .await?;
        let view = find(conn, ctx, &id).await?;
        append_event(
            conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "attachment", "created", id, &view)?,
        )
        .await?;
        Ok(view)
    }

    pub async fn get(
        &self,
        ctx: &HouseholdCtx,
        id: AttachmentId,
    ) -> Result<AttachmentView, AppError> {
        let mut conn = self.pool.acquire().await?;
        find(&mut conn, ctx, &id.to_string()).await
    }

    /// The attachment and its file content.
    pub async fn download(
        &self,
        ctx: &HouseholdCtx,
        id: AttachmentId,
    ) -> Result<(AttachmentView, Vec<u8>), AppError> {
        let view = self.get(ctx, id).await?;
        let bytes = storage::read(&self.data_dir, &view.sha256).await?;
        Ok((view, bytes))
    }

    /// Removes the attachment; the file goes too unless another attachment
    /// (in any household) has the same content.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: AttachmentId,
    ) -> Result<(), AppError> {
        let id = id.to_string();
        let mut tx = self.pool.begin().await?;
        let view = find(&mut tx, ctx, &id).await?;
        let household_id = ctx.household_id.to_string();
        sqlx::query!(
            "DELETE FROM attachments WHERE household_id = ? AND id = ?",
            household_id,
            id
        )
        .execute(&mut *tx)
        .await?;
        let still_used = sqlx::query_scalar!(
            "SELECT EXISTS (SELECT 1 FROM attachments WHERE sha256 = ?)",
            view.sha256
        )
        .fetch_one(&mut *tx)
        .await?;
        append_event(
            &mut tx,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "attachment", "deleted", id, &view)?,
        )
        .await?;
        tx.commit().await?;
        if still_used == 0 {
            storage::remove(&self.data_dir, &view.sha256).await?;
        }
        Ok(())
    }

    /// Every attachment of the household (for export), inside a transaction.
    pub async fn list_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
    ) -> Result<Vec<AttachmentView>, AppError> {
        let household_id = ctx.household_id.to_string();
        let rows = sqlx::query_as!(
            AttachmentView,
            "SELECT id, attachable_type, attachable_id, filename, title, mime_type, size_bytes, sha256, notes, created_at
             FROM attachments WHERE household_id = ? ORDER BY created_at, id",
            household_id,
        )
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows)
    }

    /// Like [`Self::get`], inside the caller's transaction.
    pub async fn get_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: AttachmentId,
    ) -> Result<AttachmentView, AppError> {
        find(conn, ctx, &id.to_string()).await
    }
}

fn check(input: &NewAttachment) -> Result<(), AppError> {
    if !ATTACHABLE_TYPES.contains(&input.attachable_type.as_str()) {
        return Err(AppError::invalid(
            "attachable_type",
            format!("must be one of {}", ATTACHABLE_TYPES.join(", ")),
        ));
    }
    if input.filename.trim().is_empty() || input.filename.contains(['/', '\\']) {
        return Err(AppError::invalid("filename", "must be a plain file name"));
    }
    if !ALLOWED_MIME_TYPES.contains(&input.mime_type.as_str()) {
        return Err(AppError::invalid(
            "mime_type",
            format!("{:?} is not an accepted file type", input.mime_type),
        ));
    }
    if input.bytes.is_empty() || input.bytes.len() > MAX_SIZE {
        return Err(AppError::invalid(
            "file",
            format!("must be between 1 byte and {} MiB", MAX_SIZE / 1024 / 1024),
        ));
    }
    Ok(())
}

/// The split, account or tag must exist in the caller's household.
async fn check_target(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    kind: &str,
    id: &str,
) -> Result<(), AppError> {
    let household_id = ctx.household_id.to_string();
    let found = match kind {
        "split" => {
            sqlx::query_scalar!(
                "SELECT EXISTS (SELECT 1 FROM splits WHERE household_id = ? AND id = ?)",
                household_id,
                id
            )
            .fetch_one(&mut *conn)
            .await?
        }
        "account" => {
            sqlx::query_scalar!(
                "SELECT EXISTS (SELECT 1 FROM accounts WHERE household_id = ? AND id = ?)",
                household_id,
                id
            )
            .fetch_one(&mut *conn)
            .await?
        }
        "bill" => {
            sqlx::query_scalar!(
                "SELECT EXISTS (SELECT 1 FROM bills WHERE household_id = ? AND id = ?)",
                household_id,
                id
            )
            .fetch_one(&mut *conn)
            .await?
        }
        _ => {
            sqlx::query_scalar!(
                "SELECT EXISTS (SELECT 1 FROM tags WHERE household_id = ? AND id = ?)",
                household_id,
                id
            )
            .fetch_one(&mut *conn)
            .await?
        }
    };
    if found == 0 {
        return Err(AppError::invalid(
            "attachable_id",
            format!("no {kind} with this ID"),
        ));
    }
    Ok(())
}

async fn find(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: &str,
) -> Result<AttachmentView, AppError> {
    let household_id = ctx.household_id.to_string();
    sqlx::query_as!(
        AttachmentView,
        "SELECT id, attachable_type, attachable_id, filename, title, mime_type, size_bytes, sha256, notes, created_at
         FROM attachments WHERE household_id = ? AND id = ?",
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}
