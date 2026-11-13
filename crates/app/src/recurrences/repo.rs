//! SQL for `recurrences`, `recurrence_repetitions` and `recurrence_splits`.

use sqlx::SqliteConnection;

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RecurrenceRow {
    pub id: String,
    pub household_id: String,
    pub title: String,
    pub description: Option<String>,
    pub first_date: String,
    pub repeat_until: Option<String>,
    pub max_occurrences: Option<i64>,
    pub apply_rules: bool,
    pub active: bool,
    pub notes: Option<String>,
    pub fired_through: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RepetitionRow {
    pub id: String,
    pub kind: String,
    pub moment: String,
    pub skip: i64,
    pub weekend_policy: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TemplateSplitRow {
    pub id: String,
    pub position: i64,
    pub description: String,
    pub source_account_id: String,
    pub destination_account_id: String,
    pub amount_minor: i64,
    pub currency_code: String,
    pub foreign_amount_minor: Option<i64>,
    pub foreign_currency_code: Option<String>,
    pub category_id: Option<String>,
    pub budget_id: Option<String>,
    pub bill_id: Option<String>,
    pub piggy_bank_id: Option<String>,
    /// A JSON array of tag IDs.
    pub tags: String,
}

pub(crate) async fn insert(
    conn: &mut SqliteConnection,
    row: &RecurrenceRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO recurrences (id, household_id, title, description, first_date, repeat_until,
            max_occurrences, apply_rules, active, notes, fired_through, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        row.household_id,
        row.title,
        row.description,
        row.first_date,
        row.repeat_until,
        row.max_occurrences,
        row.apply_rules,
        row.active,
        row.notes,
        row.fired_through,
        row.created_at,
        row.updated_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Updates every column except the ID, household, creation time and runner
/// progress.
pub(crate) async fn update(
    conn: &mut SqliteConnection,
    row: &RecurrenceRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE recurrences SET title = ?, description = ?, first_date = ?, repeat_until = ?,
            max_occurrences = ?, apply_rules = ?, active = ?, notes = ?, updated_at = ?
         WHERE household_id = ? AND id = ?",
        row.title,
        row.description,
        row.first_date,
        row.repeat_until,
        row.max_occurrences,
        row.apply_rules,
        row.active,
        row.notes,
        row.updated_at,
        row.household_id,
        row.id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn set_fired_through(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
    date: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE recurrences SET fired_through = ? WHERE household_id = ? AND id = ?",
        date,
        household_id,
        id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn get(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<Option<RecurrenceRow>, AppError> {
    let row = sqlx::query_as!(
        RecurrenceRow,
        r#"SELECT id, household_id, title, description, first_date, repeat_until, max_occurrences,
                  apply_rules AS "apply_rules: bool", active AS "active: bool", notes,
                  fired_through, created_at, updated_at
           FROM recurrences WHERE household_id = ? AND id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// One page ordered by title, plus the total; `active` filters.
pub(crate) async fn list(
    conn: &mut SqliteConnection,
    household_id: &str,
    active: Option<bool>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<RecurrenceRow>, i64), AppError> {
    let rows = sqlx::query_as!(
        RecurrenceRow,
        r#"SELECT id, household_id, title, description, first_date, repeat_until, max_occurrences,
                  apply_rules AS "apply_rules: bool", active AS "active: bool", notes,
                  fired_through, created_at, updated_at
           FROM recurrences WHERE household_id = ?1 AND (?2 IS NULL OR active = ?2)
           ORDER BY title, id LIMIT ?3 OFFSET ?4"#,
        household_id,
        active,
        limit,
        offset,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        "SELECT count(*) FROM recurrences WHERE household_id = ?1 AND (?2 IS NULL OR active = ?2)",
        household_id,
        active,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((rows, total))
}

/// IDs of every recurrence of the household (for the runner).
pub(crate) async fn all_ids(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<Vec<String>, AppError> {
    let ids = sqlx::query_scalar!(
        "SELECT id FROM recurrences WHERE household_id = ? ORDER BY id",
        household_id
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}

/// Deletes the recurrence with its repetitions and template splits. Groups
/// it created keep their `recurrence_id` as a historical marker.
pub(crate) async fn delete(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM recurrences WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn repetitions(
    conn: &mut SqliteConnection,
    household_id: &str,
    recurrence_id: &str,
) -> Result<Vec<RepetitionRow>, AppError> {
    let rows = sqlx::query_as!(
        RepetitionRow,
        "SELECT id, kind, moment, skip, weekend_policy FROM recurrence_repetitions
         WHERE household_id = ? AND recurrence_id = ? ORDER BY id",
        household_id,
        recurrence_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

pub(crate) async fn splits(
    conn: &mut SqliteConnection,
    household_id: &str,
    recurrence_id: &str,
) -> Result<Vec<TemplateSplitRow>, AppError> {
    let rows = sqlx::query_as!(
        TemplateSplitRow,
        "SELECT id, position, description, source_account_id, destination_account_id, amount_minor,
                currency_code, foreign_amount_minor, foreign_currency_code, category_id, budget_id,
                bill_id, piggy_bank_id, tags
         FROM recurrence_splits WHERE household_id = ? AND recurrence_id = ? ORDER BY position",
        household_id,
        recurrence_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// Replaces the repetitions and template splits of a recurrence.
pub(crate) async fn replace_children(
    conn: &mut SqliteConnection,
    household_id: &str,
    recurrence_id: &str,
    repetitions: &[RepetitionRow],
    splits: &[TemplateSplitRow],
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM recurrence_repetitions WHERE household_id = ? AND recurrence_id = ?",
        household_id,
        recurrence_id
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "DELETE FROM recurrence_splits WHERE household_id = ? AND recurrence_id = ?",
        household_id,
        recurrence_id
    )
    .execute(&mut *conn)
    .await?;
    for r in repetitions {
        sqlx::query!(
            "INSERT INTO recurrence_repetitions (id, household_id, recurrence_id, kind, moment, skip,
                weekend_policy) VALUES (?, ?, ?, ?, ?, ?, ?)",
            r.id,
            household_id,
            recurrence_id,
            r.kind,
            r.moment,
            r.skip,
            r.weekend_policy,
        )
        .execute(&mut *conn)
        .await?;
    }
    for s in splits {
        sqlx::query!(
            "INSERT INTO recurrence_splits (id, household_id, recurrence_id, position, description,
                source_account_id, destination_account_id, amount_minor, currency_code,
                foreign_amount_minor, foreign_currency_code, category_id, budget_id, bill_id,
                piggy_bank_id, tags)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            s.id,
            household_id,
            recurrence_id,
            s.position,
            s.description,
            s.source_account_id,
            s.destination_account_id,
            s.amount_minor,
            s.currency_code,
            s.foreign_amount_minor,
            s.foreign_currency_code,
            s.category_id,
            s.budget_id,
            s.bill_id,
            s.piggy_bank_id,
            s.tags,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
