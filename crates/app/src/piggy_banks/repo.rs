//! SQL for `piggy_banks`, `piggy_bank_accounts` and `piggy_bank_events`.

use sqlx::SqliteConnection;

use crate::error::AppError;

/// One `piggy_banks` row, as stored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PiggyBankRow {
    pub id: String,
    pub household_id: String,
    pub name: String,
    pub target_amount_minor: Option<i64>,
    pub currency_code: String,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub active: bool,
    pub sort_order: i64,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) async fn insert(
    conn: &mut SqliteConnection,
    row: &PiggyBankRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO piggy_banks (id, household_id, name, target_amount_minor, currency_code,
            start_date, target_date, active, sort_order, notes, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.household_id,
        row.name,
        row.target_amount_minor,
        row.currency_code,
        row.start_date,
        row.target_date,
        row.active,
        row.sort_order,
        row.notes,
        row.created_at,
        row.updated_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn update(
    conn: &mut SqliteConnection,
    row: &PiggyBankRow,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE piggy_banks SET name = ?, target_amount_minor = ?, start_date = ?, target_date = ?,
            active = ?, sort_order = ?, notes = ?, updated_at = ?
         WHERE household_id = ? AND id = ?",
        row.name,
        row.target_amount_minor,
        row.start_date,
        row.target_date,
        row.active,
        row.sort_order,
        row.notes,
        row.updated_at,
        row.household_id,
        row.id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn get(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<Option<PiggyBankRow>, AppError> {
    let row = sqlx::query_as!(
        PiggyBankRow,
        r#"SELECT id, household_id, name, target_amount_minor, currency_code, start_date,
                  target_date, active AS "active: bool", sort_order, notes, created_at, updated_at
           FROM piggy_banks WHERE household_id = ? AND id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// One page of piggy banks ordered by sort order and name, plus the total.
pub(crate) async fn list(
    conn: &mut SqliteConnection,
    household_id: &str,
    active: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<PiggyBankRow>, i64), AppError> {
    let rows = sqlx::query_as!(
        PiggyBankRow,
        r#"SELECT id, household_id, name, target_amount_minor, currency_code, start_date,
                  target_date, active AS "active: bool", sort_order, notes, created_at, updated_at
           FROM piggy_banks WHERE household_id = ?1 AND (?2 IS NULL OR active = ?2)
           ORDER BY sort_order, name, id LIMIT ?3 OFFSET ?4"#,
        household_id,
        active,
        limit,
        offset,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        "SELECT count(*) FROM piggy_banks WHERE household_id = ?1 AND (?2 IS NULL OR active = ?2)",
        household_id,
        active,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((rows, total))
}

/// Deletes the piggy bank; its account links and events go with it.
pub(crate) async fn delete(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM piggy_banks WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The linked accounts with the amount saved in each, in account-ID order.
pub(crate) async fn accounts(
    conn: &mut SqliteConnection,
    household_id: &str,
    piggy_bank_id: &str,
) -> Result<Vec<(String, i64)>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT pa.account_id,
                  COALESCE((SELECT SUM(e.amount_minor) FROM piggy_bank_events e
                            WHERE e.piggy_bank_id = pa.piggy_bank_id
                              AND e.account_id = pa.account_id), 0) AS "saved!: i64"
           FROM piggy_bank_accounts pa
           WHERE pa.household_id = ? AND pa.piggy_bank_id = ?
           ORDER BY pa.account_id"#,
        household_id,
        piggy_bank_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.account_id, r.saved)).collect())
}

/// Replaces the set of linked accounts.
pub(crate) async fn set_accounts(
    conn: &mut SqliteConnection,
    household_id: &str,
    piggy_bank_id: &str,
    account_ids: &[String],
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM piggy_bank_accounts WHERE household_id = ? AND piggy_bank_id = ?",
        household_id,
        piggy_bank_id
    )
    .execute(&mut *conn)
    .await?;
    for account_id in account_ids {
        sqlx::query!(
            "INSERT INTO piggy_bank_accounts (household_id, piggy_bank_id, account_id) VALUES (?, ?, ?)",
            household_id,
            piggy_bank_id,
            account_id,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// The sum of every event, and of those booked on `account_id`.
pub(crate) async fn saved(
    conn: &mut SqliteConnection,
    piggy_bank_id: &str,
    account_id: &str,
) -> Result<(i64, i64), AppError> {
    let row = sqlx::query!(
        r#"SELECT COALESCE(SUM(amount_minor), 0) AS "total!: i64",
                  COALESCE(SUM(CASE WHEN account_id = ?2 THEN amount_minor END), 0) AS "in_account!: i64"
           FROM piggy_bank_events WHERE piggy_bank_id = ?1"#,
        piggy_bank_id,
        account_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((row.total, row.in_account))
}

/// One `piggy_bank_events` row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EventRow {
    pub id: String,
    pub household_id: String,
    pub piggy_bank_id: String,
    pub account_id: String,
    pub amount_minor: i64,
    pub split_id: Option<String>,
    pub date: String,
    pub created_at: String,
}

pub(crate) async fn insert_event(
    conn: &mut SqliteConnection,
    row: &EventRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO piggy_bank_events (id, household_id, piggy_bank_id, account_id, amount_minor,
            split_id, date, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.household_id,
        row.piggy_bank_id,
        row.account_id,
        row.amount_minor,
        row.split_id,
        row.date,
        row.created_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// One page of a piggy bank's events, newest first, plus the total.
pub(crate) async fn list_events(
    conn: &mut SqliteConnection,
    household_id: &str,
    piggy_bank_id: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<EventRow>, i64), AppError> {
    let rows = sqlx::query_as!(
        EventRow,
        "SELECT id, household_id, piggy_bank_id, account_id, amount_minor, split_id, date, created_at
         FROM piggy_bank_events WHERE household_id = ? AND piggy_bank_id = ?
         ORDER BY date DESC, id DESC LIMIT ? OFFSET ?",
        household_id,
        piggy_bank_id,
        limit,
        offset,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        "SELECT count(*) FROM piggy_bank_events WHERE household_id = ? AND piggy_bank_id = ?",
        household_id,
        piggy_bank_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((rows, total))
}

/// A split's kind and two accounts, if it exists in the household.
pub(crate) async fn split_sides(
    conn: &mut SqliteConnection,
    household_id: &str,
    split_id: &str,
) -> Result<Option<(String, String, String)>, AppError> {
    let row = sqlx::query!(
        "SELECT kind, source_account_id, destination_account_id FROM splits
         WHERE household_id = ? AND id = ?",
        household_id,
        split_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|r| (r.kind, r.source_account_id, r.destination_account_id)))
}

/// Whether the piggy bank exists in the household (for template splits).
pub(crate) async fn exists(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<bool, AppError> {
    let found = sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM piggy_banks WHERE household_id = ? AND id = ?)",
        household_id,
        id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(found != 0)
}
