//! SQL for `transaction_templates`. Every query filters by household.

use sqlx::SqliteConnection;

use super::view::TemplateRow;
use crate::error::AppError;

const DUPLICATE: &str = "a template with this name exists";

/// The values of a row that a write sets.
pub(super) struct Fields<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub title: Option<&'a str>,
    pub splits: &'a str,
    pub sort_order: i64,
}

pub(super) async fn insert(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
    fields: &Fields<'_>,
    now: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO transaction_templates
            (id, household_id, name, kind, title, splits, sort_order, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        id,
        household_id,
        fields.name,
        fields.kind,
        fields.title,
        fields.splits,
        fields.sort_order,
        now,
        now,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
    Ok(())
}

pub(super) async fn update(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
    fields: &Fields<'_>,
    now: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE transaction_templates
         SET name = ?, kind = ?, title = ?, splits = ?, sort_order = ?, updated_at = ?
         WHERE household_id = ? AND id = ?",
        fields.name,
        fields.kind,
        fields.title,
        fields.splits,
        fields.sort_order,
        now,
        household_id,
        id,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
    Ok(())
}

pub(super) async fn delete(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM transaction_templates WHERE household_id = ? AND id = ?",
        household_id,
        id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(super) async fn find(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<TemplateRow, AppError> {
    sqlx::query_as!(
        TemplateRow,
        "SELECT id, name, kind, title, splits, sort_order, created_at, updated_at
         FROM transaction_templates WHERE household_id = ? AND id = ?",
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// A page of rows in display order; `limit` -1 means all.
pub(super) async fn list(
    conn: &mut SqliteConnection,
    household_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<TemplateRow>, AppError> {
    let rows = sqlx::query_as!(
        TemplateRow,
        "SELECT id, name, kind, title, splits, sort_order, created_at, updated_at
         FROM transaction_templates WHERE household_id = ?
         ORDER BY sort_order, name LIMIT ? OFFSET ?",
        household_id,
        limit,
        offset,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

pub(super) async fn count(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<i64, AppError> {
    let total = sqlx::query_scalar!(
        "SELECT count(*) FROM transaction_templates WHERE household_id = ?",
        household_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(total)
}

/// One more than the highest sort order, so a new template goes last.
pub(super) async fn next_sort_order(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<i64, AppError> {
    let next = sqlx::query_scalar!(
        r#"SELECT coalesce(max(sort_order) + 1, 0) AS "next!: i64"
           FROM transaction_templates WHERE household_id = ?"#,
        household_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(next)
}
