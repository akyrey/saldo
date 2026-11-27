//! Storing Amazon and PayPal exports (Phase 13, Track C; brief decision 2).

use std::path::{Path, PathBuf};

use saldo_core::matching::PayPalKind;
use saldo_core::{Currency, Money};
use serde_json::json;
use sqlx::{SqliteConnection, SqlitePool};

use super::parse::{AmazonRecord, PayPalRecord, SourceRow, parse_amazon, parse_paypal};
use super::repo::{self, DocumentRow, NewRow, StoredRow, Upserted};
use super::views::{DocumentView, RowFilter, SourceRowView, SourceUpload};
use crate::attachments::storage;
use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity::household_zone;
use crate::page::{Page, PageRequest};
use crate::statements::parse::Format;

const ENTITY: &str = "source_document";
/// Longest filename kept.
const MAX_FILENAME_CHARS: usize = 200;

#[derive(Clone)]
pub struct SourceService {
    pool: SqlitePool,
    clock: SharedClock,
    data_dir: PathBuf,
}

impl SourceService {
    pub fn new(pool: SqlitePool, clock: SharedClock, data_dir: PathBuf) -> Self {
        Self {
            pool,
            clock,
            data_dir,
        }
    }

    /// Where uploaded files are kept, by SHA-256, so a parser fix can read
    /// them again. Apart from attachments, which their sweep owns.
    fn folder(&self) -> PathBuf {
        self.data_dir.join("sources")
    }

    /// Stores an export: new rows are added, known ones refreshed (decision
    /// 2), and the file is kept. The same file twice is a 409.
    pub async fn upload(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: SourceUpload,
    ) -> Result<DocumentView, AppError> {
        let kind = input.kind.as_str();
        if !matches!(kind, "amazon_orders" | "paypal") {
            return Err(AppError::invalid("kind", "must be amazon_orders or paypal"));
        }
        let rows = {
            let mut conn = self.pool.acquire().await?;
            parsed_rows(&mut conn, ctx, kind, &input.bytes).await?
        };
        let household_id = ctx.household_id.to_string();
        let sha256 = storage::sha256_hex(&input.bytes);
        let mut tx = crate::db::begin(&self.pool).await?;
        if repo::sha_known(&mut tx, &household_id, &sha256).await? {
            return Err(AppError::Conflict(
                "this file was uploaded already".to_string(),
            ));
        }
        let mut document = DocumentRow {
            id: new_id(&*self.clock).to_string(),
            kind: kind.to_string(),
            filename: clean_filename(&input.filename),
            sha256,
            first_date: rows.iter().map(|r| r.date.clone()).min(),
            last_date: rows.iter().map(|r| r.date.clone()).max(),
            row_count: i64::try_from(rows.len()).map_err(|e| AppError::Internal(e.to_string()))?,
            rows_added: 0,
            rows_updated: 0,
            created_at: format_timestamp(self.clock.now()),
        };
        repo::insert_document(&mut tx, &household_id, &document, &ctx.user_id.to_string()).await?;
        for row in &rows {
            let id = new_id(&*self.clock).to_string();
            match repo::upsert_row(&mut tx, &household_id, kind, &document.id, &id, row).await? {
                Upserted::Added => document.rows_added += 1,
                Upserted::Updated => document.rows_updated += 1,
                Upserted::Unchanged => {}
            }
        }
        repo::set_counts(
            &mut tx,
            &document.id,
            document.rows_added,
            document.rows_updated,
        )
        .await?;
        storage::store_in(&self.folder(), &document.sha256, &input.bytes).await?;
        let view = document_view(document);
        let event = NewEvent::new(&*self.clock, ENTITY, "created", view.id.clone(), &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// The household's uploads, newest first.
    pub async fn documents(&self, ctx: &HouseholdCtx) -> Result<Vec<DocumentView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let rows = repo::documents(&mut conn, &ctx.household_id.to_string()).await?;
        Ok(rows.into_iter().map(document_view).collect())
    }

    /// Stored rows by date.
    pub async fn rows(
        &self,
        ctx: &HouseholdCtx,
        filter: RowFilter,
        page: PageRequest,
    ) -> Result<Page<SourceRowView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let currencies = repo::currencies(&mut conn).await?;
        let (from, to) = (
            filter.from.map(|d| d.to_string()),
            filter.to.map(|d| d.to_string()),
        );
        let (rows, total) = repo::rows(
            &mut conn,
            &ctx.household_id.to_string(),
            filter.kind.as_deref(),
            from.as_deref(),
            to.as_deref(),
            filter.unmatched_only,
            page.limit(),
            page.offset(),
        )
        .await?;
        let items = rows
            .into_iter()
            .map(|row| row_view(row, &currencies))
            .collect();
        Ok(Page::new(items, page, total))
    }

    /// Deletes an upload and the rows no other upload holds (409 when such a
    /// row explains a booked transaction).
    pub async fn delete_document(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<(), AppError> {
        let household_id = ctx.household_id.to_string();
        let mut tx = crate::db::begin(&self.pool).await?;
        let document = repo::find_document(&mut tx, &household_id, id).await?;
        repo::release_rows(&mut tx, &household_id, id).await?;
        repo::delete_document(&mut tx, &household_id, id).await?;
        let sha256 = document.sha256.clone();
        let view = document_view(document);
        let event = NewEvent::new(&*self.clock, ENTITY, "deleted", view.id.clone(), &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        let still_used = repo::sha_used(&mut tx, &sha256).await?;
        tx.commit().await?;
        if !still_used {
            remove_file(&self.folder(), &sha256).await?;
        }
        Ok(())
    }
}

/// Every upload, row and booked match of the household, for the export.
pub(crate) async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<
    (
        Vec<DocumentView>,
        Vec<SourceRowView>,
        Vec<crate::sources::SourceMatchView>,
    ),
    AppError,
> {
    let household_id = ctx.household_id.to_string();
    let documents = repo::documents(conn, &household_id)
        .await?
        .into_iter()
        .map(document_view)
        .collect();
    let currencies = repo::currencies(conn).await?;
    let (rows, _) = repo::rows(conn, &household_id, None, None, None, false, i64::MAX, 0).await?;
    let rows = rows
        .into_iter()
        .map(|row| row_view(row, &currencies))
        .collect();
    let matches = repo::booked_matches(conn, &household_id).await?;
    Ok((documents, rows, matches))
}

async fn remove_file(folder: &Path, sha256: &str) -> Result<(), AppError> {
    match tokio::fs::remove_file(folder.join(sha256)).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::Internal(format!(
            "cannot remove a source file: {error}"
        ))),
    }
}

/// The file's rows ready to store; unreadable rows are left out (the
/// parser's reasons are for the preview, not stored).
async fn parsed_rows(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    kind: &str,
    bytes: &[u8],
) -> Result<Vec<NewRow>, AppError> {
    let currencies = repo::currencies(conn).await?;
    let invalid =
        |error: crate::statements::parse::ParseError| AppError::invalid("file", error.to_string());
    if kind == "amazon_orders" {
        let zone = household_zone(conn, ctx).await?;
        let rows = parse_amazon(bytes, &currencies, zone).map_err(invalid)?;
        Ok(records(rows).iter().map(amazon_row).collect())
    } else {
        // PayPal exports CSV; an xlsx copy works too.
        let format = if bytes.starts_with(b"PK") {
            Format::Xlsx
        } else {
            Format::Csv
        };
        let rows = parse_paypal(format, bytes, &currencies).map_err(invalid)?;
        Ok(records(rows).iter().map(paypal_row).collect())
    }
}

fn records<T>(rows: Vec<SourceRow<T>>) -> Vec<T> {
    rows.into_iter()
        .filter_map(|row| match row {
            SourceRow::Record(record) => Some(record),
            SourceRow::Unsupported { .. } => None,
        })
        .collect()
}

fn amazon_row(record: &AmazonRecord) -> NewRow {
    NewRow {
        external_id: record.external_id.clone(),
        date: record.date.to_string(),
        time: None,
        amount_minor: record.amount.minor(),
        currency_code: record.amount.currency().to_string(),
        row_kind: "item".to_string(),
        group_key: Some(record.order_id.clone()),
        counterparty: None,
        text: Some(record.product.clone()).filter(|t| !t.is_empty()),
        status: Some(record.status.clone()).filter(|s| !s.is_empty()),
        excluded: record.cancelled || record.gift_card_only,
        raw: json!({ "asin": record.asin, "payment_method": record.payment_method, "website": record.website, "row": record.row }).to_string(),
    }
}

/// The stored name of a PayPal row's kind.
pub(crate) fn paypal_kind_name(kind: PayPalKind) -> &'static str {
    match kind {
        PayPalKind::Funding => "funding",
        PayPalKind::Withdrawal => "withdrawal",
        PayPalKind::Payment => "payment",
        PayPalKind::Refund => "refund",
        PayPalKind::Other => "other",
    }
}

fn paypal_row(record: &PayPalRecord) -> NewRow {
    NewRow {
        external_id: record.transaction_code.clone(),
        date: record.date.to_string(),
        time: record.time.map(|t| t.format("%H:%M:%S").to_string()),
        amount_minor: record.gross.minor(),
        currency_code: record.gross.currency().to_string(),
        row_kind: paypal_kind_name(record.kind).to_string(),
        group_key: None,
        counterparty: Some(record.name.clone()).filter(|n| !n.is_empty()),
        text: record.description.clone(),
        status: Some(record.status.clone()).filter(|s| !s.is_empty()),
        excluded: false,
        raw: json!({ "kind": record.kind_text, "timezone": record.timezone, "receipt_code": record.receipt_code, "row": record.row }).to_string(),
    }
}

fn clean_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base.chars().filter(|c| !c.is_control()).collect();
    let clean: String = clean.trim().chars().take(MAX_FILENAME_CHARS).collect();
    if clean.is_empty() {
        "export".to_string()
    } else {
        clean
    }
}

fn document_view(row: DocumentRow) -> DocumentView {
    DocumentView {
        id: row.id,
        kind: row.kind,
        filename: row.filename,
        first_date: row.first_date,
        last_date: row.last_date,
        row_count: row.row_count,
        rows_added: row.rows_added,
        rows_updated: row.rows_updated,
        created_at: row.created_at,
    }
}

fn row_view(row: StoredRow, currencies: &[Currency]) -> SourceRowView {
    let currency = currencies
        .iter()
        .find(|c| c.code().to_string() == row.currency_code);
    let amount = match (currency, saldo_core::CurrencyCode::new(&row.currency_code)) {
        (Some(currency), Ok(code)) => {
            Money::new(row.amount_minor, code).to_decimal_string(currency)
        }
        _ => row.amount_minor.to_string(),
    };
    SourceRowView {
        id: row.id,
        kind: row.kind,
        external_id: row.external_id,
        date: row.date,
        time: row.time,
        amount,
        currency: row.currency_code,
        row_kind: row.row_kind,
        group_key: row.group_key,
        counterparty: row.counterparty,
        text: row.text,
        status: row.status,
        excluded: row.excluded,
        matched: row.matched,
    }
}
