//! Statement imports as drafts: upload, review edits, discarding (Phase 13,
//! Track B). Committing and reverting are in `commit.rs`.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Days, NaiveDate};
use saldo_core::matching::statement_gap;
use saldo_core::{AccountId, Currency, Money};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqliteConnection;

use super::classify::{Known, classify, ref_keys};
use super::import_repo::{self, ImportRow, LineRow};
use super::parse::{Format, Mapping, ParsedRow, StatementLine, Unsupported, parse_statement};
use super::views::{
    ImportDetail, ImportView, LineUpdate, LineView, UploadInput, detail_of, import_view, line_view,
};
use super::{StatementService, TransferRule, account_type, profile_repo};
use crate::clock::{format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::external_refs::{BANK_STATEMENT, ExternalRefs};
use crate::page::{Page, PageRequest};

pub(super) const ENTITY: &str = "statement_import";
/// How far around the statement's dates existing transactions are loaded:
/// the widest matching window (a week late, two days early) plus a margin.
const LEDGER_MARGIN_DAYS: u64 = 10;

impl StatementService {
    /// Reads a file with a saved profile, classifies every line and stores
    /// the draft, appending `statement_import.created`.
    pub async fn upload(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: UploadInput,
    ) -> Result<ImportDetail, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let profile = profile_repo::find(&mut tx, &household_id, &input.profile_id)
            .await
            .map_err(|_| AppError::invalid("profile_id", "no such profile"))?;
        let account = input
            .account_id
            .clone()
            .or(profile.account_id.clone())
            .ok_or_else(|| {
                AppError::invalid("account_id", "choose the account the statement belongs to")
            })?;
        let currency = statement_currency(&mut tx, &household_id, &account).await?;
        let mapping: Mapping = stored(&profile.mapping)?;
        let rules: Vec<TransferRule> = stored(&profile.transfer_rules)?;
        let format = if profile.format == "csv" {
            Format::Csv
        } else {
            Format::Xlsx
        };
        let parsed = parse_statement(format, &input.bytes, &mapping, &currency)
            .map_err(|error| AppError::invalid("file", error.to_string()))?;
        let (lines, unsupported) = split_rows(parsed.rows);
        let account_id: AccountId = parse_stored(&account)?;
        let keys = ref_keys(account_id, &lines);
        let (rows, findings) = self
            .classified(&mut tx, ctx, account_id, &currency, &rules, &lines, &keys)
            .await?;

        let amounts: Vec<Money> = lines.iter().map(|l| l.amount).collect();
        let gap = match (parsed.opening_balance, parsed.closing_balance) {
            (Some(opening), Some(closing)) => statement_gap(opening, &amounts, closing)
                .map_err(|e| AppError::invalid("file", e.to_string()))?
                .map_or(Some(0), |gap| Some(gap.minor())),
            _ => None,
        };
        let row = ImportRow {
            id: new_id(&*self.clock).to_string(),
            account_id: account,
            profile_id: Some(profile.id),
            filename: input.filename.trim().chars().take(200).collect(),
            sha256: hex(&Sha256::digest(&input.bytes)),
            status: "draft".to_string(),
            currency_code: currency.code().to_string(),
            opening_balance_minor: parsed.opening_balance.map(|m| m.minor()),
            closing_balance_minor: parsed.closing_balance.map(|m| m.minor()),
            closing_date: lines
                .iter()
                .map(|l| l.booked_date)
                .max()
                .map(|d| d.to_string()),
            gap_minor: gap,
            summary: summary(&rows, &unsupported, findings).to_string(),
            created_by_user_id: Some(ctx.user_id.to_string()),
            created_at: format_timestamp(self.clock.now()),
            committed_at: None,
        };
        import_repo::insert(&mut tx, &household_id, &row).await?;
        for line in &rows {
            import_repo::insert_line(&mut tx, &row.id, line).await?;
        }
        let detail = detail_of(row, rows, &currency)?;
        self.emit_import(&mut tx, ctx, origin, &detail.import, "created")
            .await?;
        tx.commit().await?;
        Ok(detail)
    }

    /// Loads what the classification needs and runs it.
    #[allow(clippy::too_many_arguments)] // all needed; a struct would only rename them
    async fn classified(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        account_id: AccountId,
        currency: &Currency,
        rules: &[TransferRule],
        lines: &[StatementLine],
        keys: &[String],
    ) -> Result<(Vec<LineRow>, Vec<Value>), AppError> {
        let household_id = ctx.household_id.to_string();
        let account = account_id.to_string();
        let refs = ExternalRefs::load(conn, ctx, BANK_STATEMENT).await?;
        let skipped_before: BTreeSet<String> =
            import_repo::skipped_before(conn, &household_id, &account, keys)
                .await?
                .into_iter()
                .collect();
        let dates = lines
            .iter()
            .flat_map(|l| [l.booked_date, l.accounting_date]);
        let (from, to) = (dates.clone().min(), dates.max());
        let margin = Days::new(LEDGER_MARGIN_DAYS);
        let ledger = match (
            from.and_then(|d| d.checked_sub_days(margin)),
            to.and_then(|d| d.checked_add_days(margin)),
        ) {
            (Some(from), Some(to)) => {
                import_repo::ledger(
                    conn,
                    &household_id,
                    &account,
                    &from.to_string(),
                    &to.to_string(),
                )
                .await?
            }
            _ => Vec::new(),
        };
        let reconciled_through = import_repo::reconciled_through(conn, &household_id, &account)
            .await?
            .map(|d| {
                d.parse::<NaiveDate>()
                    .map_err(|_| AppError::Internal(format!("stored date {d}")))
            })
            .transpose()?;
        let accounts = import_repo::accounts(conn, &household_id).await?;
        let known = Known {
            account_id,
            currency,
            refs: &refs,
            skipped_before: &skipped_before,
            ledger: &ledger,
            reconciled_through,
            accounts: &accounts,
            transfer_rules: rules,
        };
        classify(lines, keys, &known)
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: &str) -> Result<ImportDetail, AppError> {
        let mut conn = self.pool.acquire().await?;
        let row = import_repo::find(&mut conn, &ctx.household_id.to_string(), id).await?;
        let lines = import_repo::lines(&mut conn, &row.id).await?;
        let currency = currencies::find(&mut conn, &row.currency_code, "currency").await?;
        detail_of(row, lines, &currency)
    }

    /// Imports newest first, optionally of one account.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        account_id: Option<String>,
        page: PageRequest,
    ) -> Result<Page<ImportView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let (rows, total) = import_repo::list(
            &mut conn,
            &ctx.household_id.to_string(),
            account_id.as_deref(),
            page.limit(),
            page.offset(),
        )
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let currency = currencies::find(&mut conn, &row.currency_code, "currency").await?;
            items.push(import_view(row, &currency)?);
        }
        Ok(Page::new(items, page, total))
    }

    /// A review edit of one draft line; drafts append no events.
    pub async fn update_line(
        &self,
        ctx: &HouseholdCtx,
        import_id: &str,
        position: i64,
        update: LineUpdate,
    ) -> Result<LineView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let import = draft(&mut tx, &household_id, import_id).await?;
        let mut line = import_repo::lines(&mut tx, import_id)
            .await?
            .into_iter()
            .find(|l| l.position == position)
            .ok_or(AppError::NotFound)?;
        apply_update(
            &mut tx,
            &household_id,
            &import.account_id,
            &mut line,
            update,
        )
        .await?;
        import_repo::update_line(&mut tx, import_id, &line).await?;
        let currency = currencies::find(&mut tx, &import.currency_code, "currency").await?;
        let view = line_view(line, &currency)?;
        tx.commit().await?;
        Ok(view)
    }

    /// Throws a draft away, appending `statement_import.discarded`.
    pub async fn discard(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<ImportView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let mut row = draft(&mut tx, &household_id, id).await?;
        import_repo::set_status(&mut tx, id, "discarded", None).await?;
        row.status = "discarded".to_string();
        let currency = currencies::find(&mut tx, &row.currency_code, "currency").await?;
        let view = import_view(row, &currency)?;
        self.emit_import(&mut tx, ctx, origin, &view, "discarded")
            .await?;
        tx.commit().await?;
        Ok(view)
    }

    pub(super) async fn emit_import(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        view: &ImportView,
        verb: &str,
    ) -> Result<(), AppError> {
        let event = NewEvent::new(&*self.clock, ENTITY, verb, view.id.clone(), view)?;
        append_event(conn, ctx, origin, event).await?;
        Ok(())
    }
}

/// The import, if it is still a draft (409 otherwise).
pub(super) async fn draft(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<ImportRow, AppError> {
    let row = import_repo::find(conn, household_id, id).await?;
    if row.status != "draft" {
        return Err(AppError::Conflict(format!(
            "the import is {}, not a draft",
            row.status
        )));
    }
    Ok(row)
}

async fn apply_update(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    line: &mut LineRow,
    update: LineUpdate,
) -> Result<(), AppError> {
    if let Some(action) = update.action {
        if !matches!(action.as_str(), "create" | "link" | "skip") {
            return Err(AppError::invalid("action", "must be create, link or skip"));
        }
        line.action = action;
    }
    if let Some(split_id) = update.split_id {
        if let Some(id) = &split_id
            && !import_repo::split_on_account(conn, household_id, account_id, id).await?
        {
            return Err(AppError::invalid("split_id", "not a split of this account"));
        }
        line.split_id = split_id;
    }
    if let Some(counterparty) = update.counterparty_account_id {
        if let Some(id) = &counterparty
            && (id == account_id || account_type(conn, household_id, id).await?.is_none())
        {
            return Err(AppError::invalid(
                "counterparty_account_id",
                "not another account of this household",
            ));
        }
        line.counterparty_account_id = counterparty;
    }
    if let Some(category) = update.category_id {
        if let Some(id) = &category
            && !import_repo::category_exists(conn, household_id, id).await?
        {
            return Err(AppError::invalid(
                "category_id",
                "not a category of this household",
            ));
        }
        line.category_id = category;
    }
    if line.action == "link" && line.split_id.is_none() {
        return Err(AppError::invalid(
            "split_id",
            "a link needs the split it links to",
        ));
    }
    Ok(())
}

/// The currency of the household's asset or liability account.
async fn statement_currency(
    conn: &mut SqliteConnection,
    household_id: &str,
    account: &str,
) -> Result<Currency, AppError> {
    let row = sqlx::query!(
        "SELECT type AS kind, currency_code FROM accounts WHERE household_id = ? AND id = ?",
        household_id,
        account
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| AppError::invalid("account_id", "not an account of this household"))?;
    if row.kind != "asset" && row.kind != "liability" {
        return Err(AppError::invalid(
            "account_id",
            "statements belong to asset and liability accounts",
        ));
    }
    currencies::find(conn, &row.currency_code, "account_id").await
}

fn stored<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, AppError> {
    serde_json::from_str(text).map_err(|e| AppError::Internal(format!("stored profile: {e}")))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn split_rows(rows: Vec<ParsedRow>) -> (Vec<StatementLine>, Vec<(usize, Unsupported)>) {
    let mut lines = Vec::new();
    let mut unsupported = Vec::new();
    for row in rows {
        match row {
            ParsedRow::Line(line) => lines.push(line),
            ParsedRow::Unsupported { row, reason } => unsupported.push((row, reason)),
        }
    }
    (lines, unsupported)
}

/// Why a row is not a transaction, as the review shows it.
fn reason_text(reason: &Unsupported) -> String {
    match reason {
        Unsupported::NotATransaction => "not a transaction".to_string(),
        Unsupported::Pending => "pending: the bank has not booked it yet".to_string(),
        Unsupported::NothingMoved => "nothing moved".to_string(),
        Unsupported::OtherCurrency(code) => format!("in another currency ({code})"),
        Unsupported::Skipped(reason) => reason.clone(),
        Unsupported::UnreadableDate(text) => format!("unreadable date {text:?}"),
        Unsupported::UnreadableAmount(text) => format!("unreadable amount {text:?}"),
    }
}

fn summary(rows: &[LineRow], unsupported: &[(usize, Unsupported)], findings: Vec<Value>) -> Value {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for row in rows {
        *counts.entry(row.status.as_str()).or_insert(0) += 1;
    }
    let unsupported: Vec<Value> = unsupported
        .iter()
        .map(|(row, reason)| json!({ "row": row, "reason": reason_text(reason) }))
        .collect();
    json!({ "counts": counts, "unsupported": unsupported, "findings": findings })
}
