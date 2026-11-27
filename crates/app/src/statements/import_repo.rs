//! SQL for statement imports and their lines.

use sqlx::SqliteConnection;

use crate::db::json_list;
use crate::error::AppError;

#[derive(Debug, Clone)]
pub(super) struct ImportRow {
    pub id: String,
    pub account_id: String,
    pub profile_id: Option<String>,
    pub filename: String,
    pub sha256: String,
    pub status: String,
    pub currency_code: String,
    pub opening_balance_minor: Option<i64>,
    pub closing_balance_minor: Option<i64>,
    pub closing_date: Option<String>,
    pub gap_minor: Option<i64>,
    pub summary: String,
    pub created_by_user_id: Option<String>,
    pub created_at: String,
    pub committed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct LineRow {
    pub position: i64,
    pub row_number: i64,
    pub accounting_date: String,
    pub value_date: String,
    pub booked_date: String,
    pub amount_minor: i64,
    pub currency_code: String,
    pub description: String,
    pub category: Option<String>,
    pub counterparty_name: Option<String>,
    pub counterparty_iban: Option<String>,
    pub external_id: Option<String>,
    pub ref_key: String,
    pub notes: String,
    pub parts: String,
    pub status: String,
    pub action: String,
    pub reason: Option<String>,
    pub split_id: Option<String>,
    pub created_group_id: Option<String>,
    pub counterparty_account_id: Option<String>,
    pub category_id: Option<String>,
    /// What a matched Amazon or PayPal row proposes (JSON), if any.
    pub proposal: Option<String>,
    pub use_proposal: bool,
    pub split_by_item: bool,
}

pub(super) async fn insert(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: &ImportRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO statement_imports
            (id, household_id, account_id, profile_id, filename, sha256, status, currency_code,
             opening_balance_minor, closing_balance_minor, closing_date, gap_minor, summary,
             created_by_user_id, created_at, committed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        household_id,
        row.account_id,
        row.profile_id,
        row.filename,
        row.sha256,
        row.status,
        row.currency_code,
        row.opening_balance_minor,
        row.closing_balance_minor,
        row.closing_date,
        row.gap_minor,
        row.summary,
        row.created_by_user_id,
        row.created_at,
        row.committed_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(super) async fn insert_line(
    conn: &mut SqliteConnection,
    import_id: &str,
    line: &LineRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO statement_lines
            (import_id, position, row_number, accounting_date, value_date, booked_date, amount_minor,
             currency_code, description, category, counterparty_name, counterparty_iban, external_id,
             ref_key, notes, parts, status, action, reason, split_id, created_group_id,
             counterparty_account_id, category_id, proposal, use_proposal, split_by_item)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        import_id,
        line.position,
        line.row_number,
        line.accounting_date,
        line.value_date,
        line.booked_date,
        line.amount_minor,
        line.currency_code,
        line.description,
        line.category,
        line.counterparty_name,
        line.counterparty_iban,
        line.external_id,
        line.ref_key,
        line.notes,
        line.parts,
        line.status,
        line.action,
        line.reason,
        line.split_id,
        line.created_group_id,
        line.counterparty_account_id,
        line.category_id,
        line.proposal,
        line.use_proposal,
        line.split_by_item,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(super) async fn find(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<ImportRow, AppError> {
    sqlx::query_as!(
        ImportRow,
        "SELECT id, account_id, profile_id, filename, sha256, status, currency_code,
                opening_balance_minor, closing_balance_minor, closing_date, gap_minor, summary,
                created_by_user_id, created_at, committed_at
         FROM statement_imports WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// Newest first, optionally for one account, and the total.
pub(super) async fn list(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ImportRow>, i64), AppError> {
    let rows = sqlx::query_as!(
        ImportRow,
        "SELECT id, account_id, profile_id, filename, sha256, status, currency_code,
                opening_balance_minor, closing_balance_minor, closing_date, gap_minor, summary,
                created_by_user_id, created_at, committed_at
         FROM statement_imports
         WHERE household_id = ?1 AND (?2 IS NULL OR account_id = ?2)
         ORDER BY created_at DESC, id DESC LIMIT ?3 OFFSET ?4",
        household_id,
        account_id,
        limit,
        offset
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM statement_imports WHERE household_id = ?1 AND (?2 IS NULL OR account_id = ?2)",
        household_id,
        account_id
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((rows, total))
}

pub(super) async fn lines(
    conn: &mut SqliteConnection,
    import_id: &str,
) -> Result<Vec<LineRow>, AppError> {
    Ok(sqlx::query_as!(
        LineRow,
        r#"SELECT position, row_number, accounting_date, value_date, booked_date, amount_minor,
                currency_code, description, category, counterparty_name, counterparty_iban,
                external_id, ref_key, notes, parts, status, action, reason, split_id,
                created_group_id, counterparty_account_id, category_id, proposal,
                use_proposal AS "use_proposal: bool", split_by_item AS "split_by_item: bool"
         FROM statement_lines WHERE import_id = ? ORDER BY position"#,
        import_id
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// Writes the review fields of one line.
pub(super) async fn update_line(
    conn: &mut SqliteConnection,
    import_id: &str,
    line: &LineRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE statement_lines
         SET action = ?, split_id = ?, created_group_id = ?, counterparty_account_id = ?, category_id = ?,
             use_proposal = ?, split_by_item = ?
         WHERE import_id = ? AND position = ?",
        line.action,
        line.split_id,
        line.created_group_id,
        line.counterparty_account_id,
        line.category_id,
        line.use_proposal,
        line.split_by_item,
        import_id,
        line.position,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(super) async fn set_status(
    conn: &mut SqliteConnection,
    id: &str,
    status: &str,
    committed_at: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE statement_imports SET status = ?, committed_at = COALESCE(?, committed_at) WHERE id = ?",
        status,
        committed_at,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The `ref_key`s of lines skipped on purpose in committed imports of the
/// account (brief decision 7). Lines skipped automatically (already
/// imported, or skipped before) do not count: otherwise deleting the
/// transaction later would never bring the line back (decision 11).
pub(super) async fn skipped_before(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    keys: &[String],
) -> Result<Vec<String>, AppError> {
    let list = json_list(keys)?;
    Ok(sqlx::query_scalar!(
        "SELECT DISTINCT l.ref_key FROM statement_lines l
         JOIN statement_imports i ON i.id = l.import_id
         WHERE i.household_id = ? AND i.account_id = ? AND i.status = 'committed'
           AND l.action = 'skip' AND l.status NOT IN ('duplicate', 'skipped_before')
           AND l.ref_key IN (SELECT value FROM json_each(?))",
        household_id,
        account_id,
        list
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// Whether the split belongs to the household and touches the account.
pub(super) async fn split_on_account(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    split_id: &str,
) -> Result<bool, AppError> {
    let found = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM splits
         WHERE household_id = ?1 AND id = ?2 AND (source_account_id = ?3 OR destination_account_id = ?3)",
        household_id,
        split_id,
        account_id
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(found > 0)
}

/// Whether any split of these groups is reconciled.
pub(super) async fn reconciled_in_groups(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_ids: &[String],
) -> Result<bool, AppError> {
    let list = json_list(group_ids)?;
    let found = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM splits
         WHERE household_id = ? AND reconciled = 1 AND group_id IN (SELECT value FROM json_each(?))",
        household_id,
        list
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(found > 0)
}

/// The groups among these that still exist (the owner may have deleted some).
pub(super) async fn existing_groups(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_ids: &[String],
) -> Result<Vec<String>, AppError> {
    let list = json_list(group_ids)?;
    Ok(sqlx::query_scalar!(
        "SELECT id FROM transaction_groups
         WHERE household_id = ? AND id IN (SELECT value FROM json_each(?)) ORDER BY id",
        household_id,
        list
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// The household's account of this type named `name`, ignoring case the
/// way the draft's proposal does (Unicode lowercase, not SQLite's ASCII-only
/// `NOCASE`), so a commit finds what the review showed.
pub(super) async fn named_account(
    conn: &mut SqliteConnection,
    household_id: &str,
    kind: &str,
    name: &str,
) -> Result<Option<String>, AppError> {
    let rows = sqlx::query!(
        "SELECT id, name FROM accounts WHERE household_id = ? AND type = ? ORDER BY name, id",
        household_id,
        kind
    )
    .fetch_all(&mut *conn)
    .await?;
    let wanted = name.trim().to_lowercase();
    Ok(rows
        .into_iter()
        .find(|row| row.name.trim().to_lowercase() == wanted)
        .map(|row| row.id))
}

pub(super) async fn category_exists(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<bool, AppError> {
    let found = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM categories WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(found > 0)
}
