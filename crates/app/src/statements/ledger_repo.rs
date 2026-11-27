//! SQL the classification reads: existing transactions, the reconciled-up-to
//! date and the household's accounts.

use sqlx::SqliteConnection;

use crate::error::AppError;

/// A split of a group that touches the account in the date range, unless it
/// is already linked to a line of *this* account's statements (a transfer
/// linked from the other account's statement is still a candidate).
#[derive(Debug, Clone)]
pub(super) struct LedgerRow {
    pub id: String,
    pub group_id: String,
    pub kind: String,
    pub date: String,
    pub amount_minor: i64,
    pub currency_code: String,
    pub source_account_id: String,
    pub destination_account_id: String,
    pub description: String,
}

pub(super) async fn ledger(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    from: &str,
    to: &str,
) -> Result<Vec<LedgerRow>, AppError> {
    Ok(sqlx::query_as!(
        LedgerRow,
        "SELECT s.id, s.group_id, s.kind, s.date, s.amount_minor, s.currency_code,
                s.source_account_id, s.destination_account_id, s.description
         FROM splits s
         WHERE s.household_id = ?1
           AND s.group_id IN (
               SELECT group_id FROM splits
               WHERE household_id = ?1 AND date BETWEEN ?3 AND ?4
                 AND (source_account_id = ?2 OR destination_account_id = ?2))
           AND NOT EXISTS (
               SELECT 1 FROM external_refs r
               WHERE r.household_id = ?1 AND r.source = 'bank-statement'
                 AND r.entity_type = 'split' AND r.local_id = s.id
                 AND substr(r.external_id, 1, length(?2) + 1) = ?2 || ':')
         ORDER BY s.date, s.group_id, s.position",
        household_id,
        account_id,
        from,
        to
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// The account's "reconciled up to" date: its latest reconciliation.
pub(super) async fn reconciled_through(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
) -> Result<Option<String>, AppError> {
    Ok(sqlx::query_scalar!(
        "SELECT MAX(statement_date) FROM reconciliations WHERE household_id = ? AND account_id = ?",
        household_id,
        account_id
    )
    .fetch_one(&mut *conn)
    .await?)
}

/// The household's accounts, for proposing counterparties.
#[derive(Debug, Clone)]
pub(super) struct AccountRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub iban: Option<String>,
}

pub(super) async fn accounts(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<Vec<AccountRow>, AppError> {
    Ok(sqlx::query_as!(
        AccountRow,
        r#"SELECT id, type AS "kind", name, iban FROM accounts WHERE household_id = ? ORDER BY name, id"#,
        household_id
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// A stored Amazon or PayPal row, for enriching lines (Track D).
#[derive(Debug, Clone)]
pub(super) struct SourceRowRecord {
    pub id: String,
    pub kind: String,
    pub external_id: String,
    pub date: String,
    pub time: Option<String>,
    pub amount_minor: i64,
    pub currency_code: String,
    pub row_kind: String,
    pub group_key: Option<String>,
    pub counterparty: Option<String>,
    pub text: Option<String>,
    pub excluded: bool,
}

/// The household's source rows not yet explaining a transaction (each
/// explains one at most), dated in the range, plus every Amazon item
/// of the orders named in `order_ids` whatever its date (an order ID matches
/// however late the charge came, L19).
pub(super) async fn source_rows(
    conn: &mut SqliteConnection,
    household_id: &str,
    from: &str,
    to: &str,
    order_ids: &[String],
) -> Result<Vec<SourceRowRecord>, AppError> {
    let ids = crate::db::json_list(order_ids)?;
    Ok(sqlx::query_as!(
        SourceRowRecord,
        r#"SELECT id, kind, external_id, date, time, amount_minor, currency_code, row_kind, group_key,
                  counterparty, text, excluded AS "excluded: bool"
           FROM source_rows
           WHERE household_id = ?1
             AND (date BETWEEN ?2 AND ?3
                  OR (kind = 'amazon_orders' AND group_key IN (SELECT value FROM json_each(?4))))
             AND NOT EXISTS (SELECT 1 FROM source_matches m
                             WHERE m.source_row_id = source_rows.id AND m.split_id IS NOT NULL)
           ORDER BY date, COALESCE(time, ''), rowid"#,
        household_id,
        from,
        to,
        ids
    )
    .fetch_all(&mut *conn)
    .await?)
}
