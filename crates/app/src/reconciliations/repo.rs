//! SQL for `reconciliations` and `reconciliation_splits`.

use sqlx::SqliteConnection;

use crate::error::AppError;

/// A `reconciliations` row as stored.
#[derive(Debug, Clone)]
pub(crate) struct ReconciliationRow {
    pub id: String,
    pub account_id: String,
    pub statement_date: String,
    pub statement_balance_minor: i64,
    pub currency_code: String,
    pub difference_minor: i64,
    pub difference_group_id: Option<String>,
    pub created_by_user_id: Option<String>,
    pub created_at: String,
}

/// One side of a split on the reconciled account: `amount_minor` is signed
/// (+ in, - out) in the account's currency.
#[derive(Debug, Clone)]
pub(crate) struct AccountSplitRow {
    pub id: String,
    pub group_id: String,
    pub date: String,
    pub description: String,
    pub kind: String,
    pub reconciled: bool,
    pub amount_minor: i64,
}

/// The account's type and currency, if it is the household's.
pub(crate) async fn account(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<Option<(String, String)>, AppError> {
    let row = sqlx::query!(
        r#"SELECT type AS "kind!", currency_code FROM accounts WHERE household_id = ? AND id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|r| (r.kind, r.currency_code)))
}

/// The household's Equity(Reconciliation) account, created at setup.
pub(crate) async fn reconciliation_account(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<String, AppError> {
    sqlx::query_scalar!(
        "SELECT id FROM accounts WHERE household_id = ? AND equity_role = 'reconciliation'",
        household_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| AppError::Internal("the reconciliation equity account is missing".into()))
}

/// The account's splits dated on or before `through`, oldest first.
pub(crate) async fn account_splits(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    through: &str,
) -> Result<Vec<AccountSplitRow>, AppError> {
    let rows = sqlx::query_as!(
        AccountSplitRow,
        r#"SELECT s.id AS "id!", s.group_id AS "group_id!", s.date AS "date!",
                  s.description AS "description!", s.kind AS "kind!",
                  s.reconciled AS "reconciled!: bool",
                  CASE WHEN s.destination_account_id = a.id THEN 1 ELSE -1 END
                      * CASE WHEN s.foreign_currency_code = a.currency_code
                             THEN s.foreign_amount_minor ELSE s.amount_minor END AS "amount_minor!: i64"
           FROM splits s
           JOIN accounts a ON a.household_id = s.household_id
                          AND a.id IN (s.source_account_id, s.destination_account_id)
           WHERE s.household_id = ?1 AND a.id = ?2 AND s.date <= ?3
           ORDER BY s.date, s.group_id, s.position"#,
        household_id,
        account_id,
        through,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

pub(crate) async fn find(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<ReconciliationRow, AppError> {
    sqlx::query_as!(
        ReconciliationRow,
        "SELECT id, account_id, statement_date, statement_balance_minor, currency_code, difference_minor,
                difference_group_id, created_by_user_id, created_at
         FROM reconciliations WHERE household_id = ? AND id = ?",
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// The account's reconciliations, latest statement first.
pub(crate) async fn list(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    limit: i64,
    offset: i64,
) -> Result<(Vec<ReconciliationRow>, i64), AppError> {
    let rows = sqlx::query_as!(
        ReconciliationRow,
        "SELECT id, account_id, statement_date, statement_balance_minor, currency_code, difference_minor,
                difference_group_id, created_by_user_id, created_at
         FROM reconciliations WHERE household_id = ? AND account_id = ?
         ORDER BY statement_date DESC, created_at DESC, id DESC LIMIT ? OFFSET ?",
        household_id,
        account_id,
        limit,
        offset,
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        "SELECT count(*) FROM reconciliations WHERE household_id = ? AND account_id = ?",
        household_id,
        account_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((rows, total))
}

pub(crate) async fn insert(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: &ReconciliationRow,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO reconciliations (id, household_id, account_id, statement_date, statement_balance_minor,
                                      currency_code, difference_minor, difference_group_id,
                                      created_by_user_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id, household_id, row.account_id, row.statement_date, row.statement_balance_minor,
        row.currency_code, row.difference_minor, row.difference_group_id, row.created_by_user_id,
        row.created_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn delete(conn: &mut SqliteConnection, id: &str) -> Result<(), AppError> {
    sqlx::query!("DELETE FROM reconciliations WHERE id = ?", id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Links the splits to the reconciliation and marks them reconciled.
pub(crate) async fn link(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
    split_ids: &[String],
) -> Result<(), AppError> {
    for split_id in split_ids {
        sqlx::query!(
            "INSERT INTO reconciliation_splits (reconciliation_id, split_id) VALUES (?, ?)",
            id,
            split_id,
        )
        .execute(&mut *conn)
        .await?;
        sqlx::query!(
            "UPDATE splits SET reconciled = 1 WHERE household_id = ? AND id = ?",
            household_id,
            split_id,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// The splits a reconciliation confirmed, oldest first.
pub(crate) async fn linked(conn: &mut SqliteConnection, id: &str) -> Result<Vec<String>, AppError> {
    let ids = sqlx::query_scalar!(
        "SELECT r.split_id FROM reconciliation_splits r JOIN splits s ON s.id = r.split_id
         WHERE r.reconciliation_id = ? ORDER BY s.date, s.id",
        id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}

/// Clears the reconciled flag of the splits (their links go with the row).
pub(crate) async fn clear_flags(
    conn: &mut SqliteConnection,
    household_id: &str,
    split_ids: &[String],
) -> Result<(), AppError> {
    for split_id in split_ids {
        sqlx::query!(
            "UPDATE splits SET reconciled = 0 WHERE household_id = ? AND id = ?",
            household_id,
            split_id,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Every reconciliation of the household, oldest first (for the export).
pub(crate) async fn all(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<Vec<ReconciliationRow>, AppError> {
    let rows = sqlx::query_as!(
        ReconciliationRow,
        "SELECT id, account_id, statement_date, statement_balance_minor, currency_code, difference_minor,
                difference_group_id, created_by_user_id, created_at
         FROM reconciliations WHERE household_id = ? ORDER BY account_id, statement_date, created_at",
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}
