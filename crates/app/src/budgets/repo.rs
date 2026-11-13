//! SQL for `budgets` and `budget_limits`.

use sqlx::SqliteConnection;

use crate::error::AppError;

/// A `budgets` row as stored.
#[derive(Debug, Clone)]
pub(crate) struct BudgetRow {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub sort_order: i64,
    pub notes: Option<String>,
    pub auto_type: Option<String>,
    pub auto_period: Option<String>,
    pub auto_amount_minor: Option<i64>,
    pub auto_currency_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A `budget_limits` row as stored.
#[derive(Debug, Clone)]
pub(crate) struct LimitRow {
    pub id: String,
    pub budget_id: String,
    pub start_date: String,
    pub end_date: String,
    pub amount_minor: i64,
    pub currency_code: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One withdrawal booked on a budget, as stored.
#[derive(Debug, Clone)]
pub(crate) struct SpendRow {
    pub date: String,
    pub amount_minor: i64,
    pub currency_code: String,
    pub foreign_amount_minor: Option<i64>,
    pub foreign_currency_code: Option<String>,
}

pub(crate) async fn find(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<BudgetRow, AppError> {
    sqlx::query_as!(
        BudgetRow,
        r#"SELECT id, name, active AS "active: bool", sort_order, notes, auto_type, auto_period,
                  auto_amount_minor, auto_currency_code, created_at, updated_at
           FROM budgets WHERE household_id = ? AND id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Budgets ordered by `sort_order`, then name; `active` filters when set.
pub(crate) async fn list(
    conn: &mut SqliteConnection,
    household_id: &str,
    active: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<BudgetRow>, i64), AppError> {
    let rows = sqlx::query_as!(
        BudgetRow,
        r#"SELECT id, name, active AS "active: bool", sort_order, notes, auto_type, auto_period,
                  auto_amount_minor, auto_currency_code, created_at, updated_at
           FROM budgets WHERE household_id = ?1 AND (?2 IS NULL OR active = ?2)
           ORDER BY sort_order, name LIMIT ?3 OFFSET ?4"#,
        household_id,
        active,
        limit,
        offset,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        "SELECT count(*) FROM budgets WHERE household_id = ?1 AND (?2 IS NULL OR active = ?2)",
        household_id,
        active,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((rows, total))
}

/// Every budget of the household (for export and auto-budgets).
pub(crate) async fn all(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<Vec<BudgetRow>, AppError> {
    let rows = sqlx::query_as!(
        BudgetRow,
        r#"SELECT id, name, active AS "active: bool", sort_order, notes, auto_type, auto_period,
                  auto_amount_minor, auto_currency_code, created_at, updated_at
           FROM budgets WHERE household_id = ? ORDER BY sort_order, name"#,
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

pub(crate) async fn insert(
    conn: &mut SqliteConnection,
    row: &BudgetRow,
    household_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO budgets (id, household_id, name, active, sort_order, notes, auto_type, auto_period,
                              auto_amount_minor, auto_currency_code, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id, household_id, row.name, row.active, row.sort_order, row.notes, row.auto_type,
        row.auto_period, row.auto_amount_minor, row.auto_currency_code, row.created_at, row.updated_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn update(
    conn: &mut SqliteConnection,
    row: &BudgetRow,
    household_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE budgets SET name = ?, active = ?, sort_order = ?, notes = ?, auto_type = ?, auto_period = ?,
                auto_amount_minor = ?, auto_currency_code = ?, updated_at = ?
         WHERE household_id = ? AND id = ?",
        row.name, row.active, row.sort_order, row.notes, row.auto_type, row.auto_period,
        row.auto_amount_minor, row.auto_currency_code, row.updated_at, household_id, row.id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Clears the budget from its splits, then deletes it (its limits cascade).
pub(crate) async fn delete(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE splits SET budget_id = NULL WHERE household_id = ? AND budget_id = ?",
        household_id,
        id,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "DELETE FROM budgets WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn find_limit(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<LimitRow, AppError> {
    sqlx::query_as!(
        LimitRow,
        "SELECT id, budget_id, start_date, end_date, amount_minor, currency_code, notes, created_at, updated_at
         FROM budget_limits WHERE household_id = ? AND id = ?",
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// The budget's limits that overlap `from..=to` (each bound optional),
/// oldest first.
pub(crate) async fn limits(
    conn: &mut SqliteConnection,
    household_id: &str,
    budget_id: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<Vec<LimitRow>, AppError> {
    let rows = sqlx::query_as!(
        LimitRow,
        "SELECT id, budget_id, start_date, end_date, amount_minor, currency_code, notes, created_at, updated_at
         FROM budget_limits
         WHERE household_id = ?1 AND budget_id = ?2
           AND (?3 IS NULL OR end_date >= ?3) AND (?4 IS NULL OR start_date <= ?4)
         ORDER BY start_date, currency_code, id",
        household_id,
        budget_id,
        from,
        to,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// The limit of this budget with the latest end date, if any.
pub(crate) async fn last_limit(
    conn: &mut SqliteConnection,
    household_id: &str,
    budget_id: &str,
) -> Result<Option<LimitRow>, AppError> {
    let row = sqlx::query_as!(
        LimitRow,
        "SELECT id, budget_id, start_date, end_date, amount_minor, currency_code, notes, created_at, updated_at
         FROM budget_limits WHERE household_id = ? AND budget_id = ?
         ORDER BY end_date DESC, start_date DESC, id DESC LIMIT 1",
        household_id,
        budget_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// Whether another limit of the budget in `currency` overlaps the dates.
pub(crate) async fn overlapping_limit(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: &LimitRow,
) -> Result<bool, AppError> {
    let found = sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM budget_limits
           WHERE household_id = ? AND budget_id = ? AND currency_code = ? AND id <> ?
             AND start_date <= ? AND end_date >= ?)",
        household_id,
        row.budget_id,
        row.currency_code,
        row.id,
        row.end_date,
        row.start_date,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(found != 0)
}

pub(crate) async fn insert_limit(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: &LimitRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO budget_limits (id, household_id, budget_id, start_date, end_date, amount_minor,
                                    currency_code, notes, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id, household_id, row.budget_id, row.start_date, row.end_date, row.amount_minor,
        row.currency_code, row.notes, row.created_at, row.updated_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn update_limit(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: &LimitRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE budget_limits SET start_date = ?, end_date = ?, amount_minor = ?, currency_code = ?,
                notes = ?, updated_at = ?
         WHERE household_id = ? AND id = ?",
        row.start_date, row.end_date, row.amount_minor, row.currency_code, row.notes,
        row.updated_at, household_id, row.id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn delete_limit(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM budget_limits WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The withdrawals booked on the budget between two dates (inclusive).
pub(crate) async fn spends(
    conn: &mut SqliteConnection,
    household_id: &str,
    budget_id: &str,
    from: &str,
    to: &str,
) -> Result<Vec<SpendRow>, AppError> {
    let rows = sqlx::query_as!(
        SpendRow,
        "SELECT date, amount_minor, currency_code, foreign_amount_minor, foreign_currency_code
         FROM splits
         WHERE household_id = ? AND budget_id = ? AND kind = 'withdrawal' AND date >= ? AND date <= ?",
        household_id,
        budget_id,
        from,
        to,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// The IDs among `ids_json` (a JSON array) that are budgets of the household.
pub(crate) async fn existing(
    conn: &mut SqliteConnection,
    household_id: &str,
    ids_json: &str,
) -> Result<Vec<String>, AppError> {
    let ids = sqlx::query_scalar!(
        "SELECT id FROM budgets WHERE household_id = ? AND id IN (SELECT value FROM json_each(?))",
        household_id,
        ids_json,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}
