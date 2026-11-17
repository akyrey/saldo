//! Reconciliation: confirming an account's splits against a bank statement's
//! date and closing balance, booking any difference as a reconciliation
//! transaction (ARCHITECTURE.md 5.4c, Phase 9).

mod repo;
mod view;

use std::collections::HashSet;

use chrono::NaiveDate;
use saldo_core::{AccountId, Currency, GroupId, Money, reconcile};
use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::page::{Page, PageRequest};
use crate::transactions::{SplitInput, TransactionInput, TransactionService};
use repo::{AccountSplitRow, ReconciliationRow};
use view::view_of;
pub use view::{DraftSplitView, ReconciliationDraftView, ReconciliationView};

const ENTITY: &str = "reconciliation";

/// A reconciliation as the client sends it.
#[derive(Debug, Clone)]
pub struct NewReconciliation {
    pub statement_date: NaiveDate,
    /// The statement's closing balance, a decimal string in the account's
    /// currency (incoming minus outgoing: negative for money owed).
    pub statement_balance: String,
    /// Unreconciled splits of the account, dated on or before the statement.
    pub split_ids: Vec<String>,
    /// Book a difference as a reconciliation transaction instead of refusing.
    pub book_difference: bool,
}

#[derive(Clone)]
pub struct ReconciliationService {
    pool: SqlitePool,
    clock: SharedClock,
    transactions: TransactionService,
}

impl ReconciliationService {
    pub fn new(pool: SqlitePool, clock: SharedClock, transactions: TransactionService) -> Self {
        Self {
            pool,
            clock,
            transactions,
        }
    }

    pub async fn draft(
        &self,
        ctx: &HouseholdCtx,
        account_id: AccountId,
        statement_date: NaiveDate,
    ) -> Result<ReconciliationDraftView, AppError> {
        let mut conn = self.pool.acquire().await?;
        let household_id = ctx.household_id.to_string();
        let account = account_id.to_string();
        let currency = reconcilable(&mut conn, &household_id, &account).await?;
        let latest = latest(&mut conn, &household_id, &account).await?;
        let rows = repo::account_splits(
            &mut conn,
            &household_id,
            &account,
            &statement_date.to_string(),
        )
        .await?;
        let cleared: i64 = rows
            .iter()
            .filter(|r| is_cleared(r))
            .map(|r| r.amount_minor)
            .sum();
        let money = |minor| Money::new(minor, currency.code()).to_decimal_string(&currency);
        let splits = rows
            .iter()
            .filter(|r| !is_cleared(r))
            .map(|r| DraftSplitView {
                id: r.id.clone(),
                group_id: r.group_id.clone(),
                date: r.date.clone(),
                description: r.description.clone(),
                amount: money(r.amount_minor),
            })
            .collect();
        Ok(ReconciliationDraftView {
            account_id: account,
            currency: currency.code().to_string(),
            statement_date: statement_date.to_string(),
            reconciled_through: latest.map(|r| r.statement_date),
            cleared_balance: money(cleared),
            splits,
        })
    }

    /// Confirms the splits; with a difference, books it (when asked) as a
    /// reconciliation transaction dated on the statement. One database
    /// transaction, one `reconciliation.created` event (plus the difference's
    /// `transaction.created`).
    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        account_id: AccountId,
        input: NewReconciliation,
    ) -> Result<ReconciliationView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let account = account_id.to_string();
        let currency = reconcilable(&mut tx, &household_id, &account).await?;
        let balance = Money::parse_decimal(&input.statement_balance, &currency)
            .map_err(|e| AppError::invalid("statement_balance", e.to_string()))?;
        let date = input.statement_date.to_string();
        let last = latest(&mut tx, &household_id, &account).await?;
        if let Some(last) = last.filter(|last| date < last.statement_date) {
            return Err(AppError::invalid(
                "statement_date",
                format!(
                    "must not be before the last reconciliation ({})",
                    last.statement_date
                ),
            ));
        }
        let rows = repo::account_splits(&mut tx, &household_id, &account, &date).await?;
        let selected = checked_selection(&rows, &input.split_ids)?;
        let cleared: Vec<Money> = rows
            .iter()
            .filter(|r| is_cleared(r) || selected.contains(r.id.as_str()))
            .map(|r| Money::new(r.amount_minor, currency.code()))
            .collect();
        let difference =
            reconcile(balance, &cleared).map_err(|e| AppError::Internal(e.to_string()))?;
        let mut split_ids: Vec<String> = input.split_ids.clone();
        let mut difference_group_id = None;
        if let Some(diff) = difference {
            if !input.book_difference {
                return Err(AppError::invalid(
                    "statement_balance",
                    format!(
                        "differs from the selected transactions by {}; select more or book the difference",
                        diff.to_decimal_string(&currency)
                    ),
                ));
            }
            let group = self
                .book(
                    &mut tx,
                    ctx,
                    origin,
                    &account,
                    input.statement_date,
                    diff,
                    &currency,
                )
                .await?;
            split_ids.extend(group.splits.iter().map(|s| s.id.clone()));
            difference_group_id = Some(group.id);
        }
        let row = ReconciliationRow {
            id: new_id(&*self.clock).to_string(),
            account_id: account,
            statement_date: date,
            statement_balance_minor: balance.minor(),
            currency_code: currency.code().to_string(),
            difference_minor: difference.map_or(0, |d| d.minor()),
            difference_group_id,
            created_by_user_id: Some(ctx.user_id.to_string()),
            created_at: format_timestamp(self.clock.now()),
        };
        repo::insert(&mut tx, &household_id, &row).await?;
        repo::link(&mut tx, &household_id, &row.id, &split_ids).await?;
        let view = view_of(&mut tx, row).await?;
        self.emit(&mut tx, ctx, origin, &view, "created").await?;
        tx.commit().await?;
        Ok(view)
    }

    /// The difference as a reconciliation transaction on the statement date.
    #[allow(clippy::too_many_arguments)] // all needed; a struct would only rename them
    async fn book(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        account: &str,
        date: NaiveDate,
        difference: Money,
        currency: &Currency,
    ) -> Result<crate::transactions::TransactionGroupView, AppError> {
        let equity = repo::reconciliation_account(conn, &ctx.household_id.to_string()).await?;
        // More on the statement: money came in from equity; less: it went out.
        let (source, destination) = if difference.is_positive() {
            (equity, account.to_string())
        } else {
            (account.to_string(), equity)
        };
        let amount = Money::new(difference.minor().abs(), currency.code());
        let split = SplitInput {
            id: None,
            date,
            description: "Reconciliation difference".to_string(),
            source_id: parse_stored(&source)?,
            destination_id: parse_stored(&destination)?,
            amount: amount.to_decimal_string(currency),
            currency: None,
            foreign_amount: None,
            foreign_currency: None,
            category_id: None,
            budget_id: None,
            bill_id: None,
            tag_ids: Vec::new(),
            reconciled: true,
            notes: None,
            meta: serde_json::Map::new(),
        };
        let input = TransactionInput {
            title: Some("Reconciliation".to_string()),
            splits: vec![split],
        };
        self.transactions
            .write_create_in(conn, ctx, origin, input)
            .await
    }

    /// Undoes the account's latest reconciliation: its splits are not
    /// reconciled any more and its difference transaction is deleted.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let row = repo::find(&mut tx, &household_id, id).await?;
        let last = latest(&mut tx, &household_id, &row.account_id).await?;
        if last.as_ref().map(|r| r.id.as_str()) != Some(id) {
            return Err(AppError::Conflict(
                "only the account's latest reconciliation can be undone".to_string(),
            ));
        }
        let view = view_of(&mut tx, row.clone()).await?;
        repo::clear_flags(&mut tx, &household_id, &view.split_ids).await?;
        if let Some(group_id) = &row.difference_group_id {
            let group_id: GroupId = parse_stored(group_id)?;
            self.transactions
                .delete_in(&mut tx, ctx, origin, group_id)
                .await?;
        }
        repo::delete(&mut tx, id).await?;
        self.emit(&mut tx, ctx, origin, &view, "deleted").await?;
        tx.commit().await?;
        Ok(())
    }

    /// The account's reconciliations, latest statement first.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        account_id: AccountId,
        page: PageRequest,
    ) -> Result<Page<ReconciliationView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let household_id = ctx.household_id.to_string();
        let account = account_id.to_string();
        reconcilable(&mut conn, &household_id, &account).await?;
        let (rows, total) = repo::list(
            &mut conn,
            &household_id,
            &account,
            page.limit(),
            page.offset(),
        )
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(view_of(&mut conn, row).await?);
        }
        Ok(Page::new(items, page, total))
    }

    async fn emit(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        view: &ReconciliationView,
        verb: &str,
    ) -> Result<(), AppError> {
        let event = NewEvent::new(&*self.clock, ENTITY, verb, view.id.clone(), view)?;
        append_event(conn, ctx, origin, event).await?;
        Ok(())
    }
}

/// Reconciled already, or an opening balance (always counted as confirmed).
fn is_cleared(row: &AccountSplitRow) -> bool {
    row.reconciled || row.kind == "opening_balance"
}

/// The selected IDs, each an unreconciled split of the account up to the date.
fn checked_selection<'a>(
    rows: &[AccountSplitRow],
    split_ids: &'a [String],
) -> Result<HashSet<&'a str>, AppError> {
    let mut selected = HashSet::new();
    for id in split_ids {
        let fits = rows.iter().any(|r| &r.id == id && !is_cleared(r));
        if !fits || !selected.insert(id.as_str()) {
            return Err(AppError::invalid(
                "split_ids",
                format!(
                    "{id} is not an unreconciled split of this account up to the statement date (or is named twice)"
                ),
            ));
        }
    }
    Ok(selected)
}

/// The account's currency if it is the household's asset or liability
/// account (404 when not the household's, 422 for another type).
async fn reconcilable(
    conn: &mut SqliteConnection,
    household_id: &str,
    account: &str,
) -> Result<Currency, AppError> {
    let (kind, code) = repo::account(conn, household_id, account)
        .await?
        .ok_or(AppError::NotFound)?;
    if kind != "asset" && kind != "liability" {
        return Err(AppError::invalid(
            "account_id",
            "only asset and liability accounts can be reconciled",
        ));
    }
    currencies::find(conn, &code, "currency").await
}

async fn latest(
    conn: &mut SqliteConnection,
    household_id: &str,
    account: &str,
) -> Result<Option<ReconciliationRow>, AppError> {
    let (mut rows, _) = repo::list(conn, household_id, account, 1, 0).await?;
    Ok(rows.pop())
}

/// Every reconciliation of the household with its splits, for the export.
pub(crate) async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<ReconciliationView>, AppError> {
    let rows = repo::all(conn, &ctx.household_id.to_string()).await?;
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        views.push(view_of(conn, row).await?);
    }
    Ok(views)
}
