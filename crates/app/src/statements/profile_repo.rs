//! SQL for statement profiles.

use sqlx::SqliteConnection;

use crate::error::AppError;

/// A stored profile; the JSON columns as text.
#[derive(Debug, Clone)]
pub(super) struct ProfileRow {
    pub id: String,
    pub name: String,
    pub account_id: Option<String>,
    pub format: String,
    pub mapping: String,
    pub part_accounts: String,
    pub transfer_rules: String,
    pub created_at: String,
    pub updated_at: String,
}

const DUPLICATE: &str = "a profile with this name already exists";

pub(super) async fn insert(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: &ProfileRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO statement_profiles
            (id, household_id, name, account_id, format, mapping, part_accounts, transfer_rules, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        household_id,
        row.name,
        row.account_id,
        row.format,
        row.mapping,
        row.part_accounts,
        row.transfer_rules,
        row.created_at,
        row.updated_at,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
    Ok(())
}

pub(super) async fn update(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: &ProfileRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE statement_profiles
         SET name = ?, account_id = ?, format = ?, mapping = ?, part_accounts = ?, transfer_rules = ?, updated_at = ?
         WHERE household_id = ? AND id = ?",
        row.name,
        row.account_id,
        row.format,
        row.mapping,
        row.part_accounts,
        row.transfer_rules,
        row.updated_at,
        household_id,
        row.id,
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
        "DELETE FROM statement_profiles WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(super) async fn find(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<ProfileRow, AppError> {
    sqlx::query_as!(
        ProfileRow,
        "SELECT id, name, account_id, format, mapping, part_accounts, transfer_rules, created_at, updated_at
         FROM statement_profiles WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

pub(super) async fn list(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<Vec<ProfileRow>, AppError> {
    Ok(sqlx::query_as!(
        ProfileRow,
        "SELECT id, name, account_id, format, mapping, part_accounts, transfer_rules, created_at, updated_at
         FROM statement_profiles WHERE household_id = ? ORDER BY name, id",
        household_id
    )
    .fetch_all(&mut *conn)
    .await?)
}
