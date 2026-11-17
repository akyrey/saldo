//! SQL for subscription suggestions: past payments and dismissals.

use sqlx::SqliteConnection;

use crate::error::AppError;

/// A withdrawal as the detector needs it, with the counterparty's name.
#[derive(Debug, Clone)]
pub(crate) struct PaymentRow {
    pub id: String,
    pub date: String,
    pub amount_minor: i64,
    pub currency_code: String,
    pub destination_account_id: String,
    pub destination_name: String,
    pub has_bill: bool,
}

/// One of the splits named when accepting a suggestion, as stored.
#[derive(Debug, Clone)]
pub(crate) struct NamedSplitRow {
    pub id: String,
    pub group_id: String,
    pub kind: String,
    pub date: String,
    pub destination_account_id: String,
    pub currency_code: String,
    pub bill_id: Option<String>,
}

/// Withdrawals dated `from..=to` paid from accounts of `owner` (any owner
/// when `None`), leaving out dismissed counterparty and currency pairs.
pub(crate) async fn payments(
    conn: &mut SqliteConnection,
    household_id: &str,
    owner: Option<&str>,
    from: &str,
    to: &str,
) -> Result<Vec<PaymentRow>, AppError> {
    let rows = sqlx::query_as!(
        PaymentRow,
        r#"SELECT s.id AS "id!", s.date AS "date!", s.amount_minor AS "amount_minor!",
                  s.currency_code AS "currency_code!", s.destination_account_id AS "destination_account_id!",
                  d.name AS "destination_name!", s.bill_id IS NOT NULL AS "has_bill!: bool"
           FROM splits s
           JOIN accounts src ON src.household_id = s.household_id AND src.id = s.source_account_id
           JOIN accounts d ON d.household_id = s.household_id AND d.id = s.destination_account_id
           WHERE s.household_id = ?1 AND s.kind = 'withdrawal' AND s.date >= ?3 AND s.date <= ?4
             AND (?2 IS NULL OR src.owner_user_id = ?2)
             AND NOT EXISTS (SELECT 1 FROM subscription_dismissals x
                             WHERE x.household_id = s.household_id
                               AND x.account_id = s.destination_account_id
                               AND x.currency_code = s.currency_code)"#,
        household_id,
        owner,
        from,
        to,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// The household's splits among `ids_json` (a JSON array).
pub(crate) async fn named_splits(
    conn: &mut SqliteConnection,
    household_id: &str,
    ids_json: &str,
) -> Result<Vec<NamedSplitRow>, AppError> {
    let rows = sqlx::query_as!(
        NamedSplitRow,
        "SELECT id, group_id, kind, date, destination_account_id, currency_code, bill_id
         FROM splits WHERE household_id = ? AND id IN (SELECT value FROM json_each(?))
         ORDER BY date, id",
        household_id,
        ids_json,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// Inserts a dismissal; `false` when it already existed.
pub(crate) async fn insert_dismissal(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    currency: &str,
    now: &str,
) -> Result<bool, AppError> {
    let result = sqlx::query!(
        "INSERT INTO subscription_dismissals (household_id, account_id, currency_code, created_at)
         VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
        household_id,
        account_id,
        currency,
        now,
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Deletes a dismissal; `false` when there was none.
pub(crate) async fn delete_dismissal(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    currency: &str,
) -> Result<bool, AppError> {
    let result = sqlx::query!(
        "DELETE FROM subscription_dismissals
         WHERE household_id = ? AND account_id = ? AND currency_code = ?",
        household_id,
        account_id,
        currency,
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Every dismissal of the household, oldest first (for the export).
pub(crate) async fn dismissals(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<Vec<(String, String, String)>, AppError> {
    let rows = sqlx::query!(
        "SELECT account_id, currency_code, created_at FROM subscription_dismissals
         WHERE household_id = ? ORDER BY created_at, account_id",
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.account_id, r.currency_code, r.created_at))
        .collect())
}

/// The type of the household's account `id`, if it exists.
pub(crate) async fn account_type(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<Option<String>, AppError> {
    let found = sqlx::query_scalar!(
        "SELECT type FROM accounts WHERE household_id = ? AND id = ?",
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(found)
}
