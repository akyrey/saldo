//! SQL for source documents and rows.

use saldo_core::{Currency, CurrencyCode};
use sqlx::SqliteConnection;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub(crate) struct DocumentRow {
    pub id: String,
    pub kind: String,
    pub filename: String,
    pub sha256: String,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
    pub row_count: i64,
    pub rows_added: i64,
    pub rows_updated: i64,
    pub created_at: String,
}

/// A row as the parsers deliver it, before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewRow {
    pub external_id: String,
    pub date: String,
    pub time: Option<String>,
    pub amount_minor: i64,
    pub currency_code: String,
    pub row_kind: String,
    pub group_key: Option<String>,
    pub counterparty: Option<String>,
    pub text: Option<String>,
    pub status: Option<String>,
    pub excluded: bool,
    pub raw: String,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredRow {
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
    pub status: Option<String>,
    pub excluded: bool,
    pub matched: bool,
}

/// Every currency Saldo knows, for reading amounts in a row's currency.
pub(crate) async fn currencies(conn: &mut SqliteConnection) -> Result<Vec<Currency>, AppError> {
    let rows = sqlx::query!("SELECT code, decimal_places FROM currencies ORDER BY code")
        .fetch_all(&mut *conn)
        .await?;
    rows.into_iter()
        .map(|row| {
            let code =
                CurrencyCode::new(&row.code).map_err(|e| AppError::Internal(e.to_string()))?;
            let places =
                u8::try_from(row.decimal_places).map_err(|e| AppError::Internal(e.to_string()))?;
            Currency::new(code, places).map_err(|e| AppError::Internal(e.to_string()))
        })
        .collect()
}

pub(crate) async fn insert_document(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: &DocumentRow,
    user_id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO source_documents
            (id, household_id, kind, filename, sha256, first_date, last_date, row_count, rows_added, rows_updated, created_by_user_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id,
        household_id,
        row.kind,
        row.filename,
        row.sha256,
        row.first_date,
        row.last_date,
        row.row_count,
        row.rows_added,
        row.rows_updated,
        user_id,
        row.created_at,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::conflict_on_unique(e, "this file was uploaded already"))?;
    Ok(())
}

pub(crate) async fn set_counts(
    conn: &mut SqliteConnection,
    id: &str,
    added: i64,
    updated: i64,
) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE source_documents SET rows_added = ?, rows_updated = ? WHERE id = ?",
        added,
        updated,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Whether the household already stored a file with this content.
pub(crate) async fn sha_known(
    conn: &mut SqliteConnection,
    household_id: &str,
    sha256: &str,
) -> Result<bool, AppError> {
    let found = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM source_documents WHERE household_id = ? AND sha256 = ?",
        household_id,
        sha256
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(found > 0)
}

/// Whether any household still keeps a file with this content on disk.
pub(crate) async fn sha_used(conn: &mut SqliteConnection, sha256: &str) -> Result<bool, AppError> {
    let found = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM source_documents WHERE sha256 = ?",
        sha256
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(found > 0)
}

pub(crate) async fn documents(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<Vec<DocumentRow>, AppError> {
    Ok(sqlx::query_as!(
        DocumentRow,
        "SELECT id, kind, filename, sha256, first_date, last_date, row_count, rows_added, rows_updated, created_at
         FROM source_documents WHERE household_id = ? ORDER BY created_at DESC, id DESC",
        household_id
    )
    .fetch_all(&mut *conn)
    .await?)
}

pub(crate) async fn find_document(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<DocumentRow, AppError> {
    sqlx::query_as!(
        DocumentRow,
        "SELECT id, kind, filename, sha256, first_date, last_date, row_count, rows_added, rows_updated, created_at
         FROM source_documents WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

/// What an upsert did with one row.
pub(crate) enum Upserted {
    Added,
    Updated,
    Unchanged,
}

/// Inserts a new row, or refreshes a known one (same kind and external ID)
/// and records the upload as the latest that holds it.
pub(crate) async fn upsert_row(
    conn: &mut SqliteConnection,
    household_id: &str,
    kind: &str,
    document_id: &str,
    new_id: &str,
    row: &NewRow,
) -> Result<Upserted, AppError> {
    let existing = sqlx::query!(
        "SELECT id, date, time, amount_minor, currency_code, row_kind, group_key, counterparty, text, status, excluded, raw
         FROM source_rows WHERE household_id = ? AND kind = ? AND external_id = ?",
        household_id,
        kind,
        row.external_id
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(old) = existing else {
        sqlx::query!(
            "INSERT INTO source_rows
                (id, household_id, kind, external_id, document_id, last_document_id, date, time, amount_minor,
                 currency_code, row_kind, group_key, counterparty, text, status, excluded, raw)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            new_id, household_id, kind, row.external_id, document_id, document_id, row.date, row.time,
            row.amount_minor, row.currency_code, row.row_kind, row.group_key, row.counterparty, row.text,
            row.status, row.excluded, row.raw,
        )
        .execute(&mut *conn)
        .await?;
        return Ok(Upserted::Added);
    };
    let same = old.date == row.date
        && old.time == row.time
        && old.amount_minor == row.amount_minor
        && old.currency_code == row.currency_code
        && old.row_kind == row.row_kind
        && old.group_key == row.group_key
        && old.counterparty == row.counterparty
        && old.text == row.text
        && old.status == row.status
        && (old.excluded != 0) == row.excluded
        && old.raw == row.raw;
    sqlx::query!(
        "UPDATE source_rows
         SET last_document_id = ?, date = ?, time = ?, amount_minor = ?, currency_code = ?, row_kind = ?,
             group_key = ?, counterparty = ?, text = ?, status = ?, excluded = ?, raw = ?
         WHERE id = ?",
        document_id, row.date, row.time, row.amount_minor, row.currency_code, row.row_kind, row.group_key,
        row.counterparty, row.text, row.status, row.excluded, row.raw, old.id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(if same {
        Upserted::Unchanged
    } else {
        Upserted::Updated
    })
}

/// Rows newest last, with the filters; and the total.
#[allow(clippy::too_many_arguments)] // the filters, then paging
pub(crate) async fn rows(
    conn: &mut SqliteConnection,
    household_id: &str,
    kind: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    unmatched_only: bool,
    limit: i64,
    offset: i64,
) -> Result<(Vec<StoredRow>, i64), AppError> {
    let items = sqlx::query_as!(
        StoredRow,
        r#"SELECT r.id, r.kind, r.external_id, r.date, r.time, r.amount_minor, r.currency_code, r.row_kind,
                  r.group_key, r.counterparty, r.text, r.status, r.excluded AS "excluded: bool",
                  EXISTS (SELECT 1 FROM source_matches m WHERE m.source_row_id = r.id) AS "matched!: bool"
           FROM source_rows r
           WHERE r.household_id = ?1 AND (?2 IS NULL OR r.kind = ?2)
             AND (?3 IS NULL OR r.date >= ?3) AND (?4 IS NULL OR r.date <= ?4)
             AND (?5 = 0 OR NOT EXISTS (SELECT 1 FROM source_matches m WHERE m.source_row_id = r.id))
           ORDER BY r.date, COALESCE(r.time, ''), r.rowid
           LIMIT ?6 OFFSET ?7"#,
        household_id, kind, from, to, unmatched_only, limit, offset
    )
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM source_rows r
         WHERE r.household_id = ?1 AND (?2 IS NULL OR r.kind = ?2)
           AND (?3 IS NULL OR r.date >= ?3) AND (?4 IS NULL OR r.date <= ?4)
           AND (?5 = 0 OR NOT EXISTS (SELECT 1 FROM source_matches m WHERE m.source_row_id = r.id))",
        household_id, kind, from, to, unmatched_only
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((items, total))
}

/// Gives up one document's hold on its rows: a row another document holds
/// moves to it; a row only this document holds is deleted, unless it
/// explains a committed transaction (then the whole delete is refused).
pub(crate) async fn release_rows(
    conn: &mut SqliteConnection,
    household_id: &str,
    document_id: &str,
) -> Result<(), AppError> {
    let committed = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM source_rows r
         WHERE r.household_id = ?1 AND r.document_id = ?2 AND r.last_document_id = ?2
           AND EXISTS (SELECT 1 FROM source_matches m WHERE m.source_row_id = r.id AND m.split_id IS NOT NULL)",
        household_id,
        document_id
    )
    .fetch_one(&mut *conn)
    .await?;
    if committed > 0 {
        return Err(AppError::Conflict(
            "rows of this file explain booked transactions; upload a newer export instead"
                .to_string(),
        ));
    }
    sqlx::query!(
        "DELETE FROM source_rows WHERE household_id = ?1 AND document_id = ?2 AND last_document_id = ?2",
        household_id,
        document_id
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE source_rows SET last_document_id = document_id WHERE household_id = ?1 AND last_document_id = ?2",
        household_id,
        document_id
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE source_rows SET document_id = last_document_id WHERE household_id = ?1 AND document_id = ?2",
        household_id,
        document_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn delete_document(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM source_documents WHERE household_id = ? AND id = ?",
        household_id,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}
