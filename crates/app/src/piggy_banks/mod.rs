//! Piggy banks: savings goals inside asset accounts. The saved amount is the
//! sum of the piggy bank's events (see `events.rs`).

mod events;
pub(crate) mod repo;
mod view;

use std::collections::BTreeSet;

use chrono::NaiveDate;
use saldo_core::{AccountId, Currency, Money, PiggyBankId};
use sqlx::{SqliteConnection, SqlitePool};

use crate::accounts::repo as account_repo;
use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::page::{Page, PageRequest};
use crate::{currencies, identity};

pub use events::{NewPiggyBankEvent, PiggyBankEventView};
pub(crate) use events::{add_for_split_in, all_in};
use repo::PiggyBankRow;
use view::view_of;
pub use view::{PiggyBankAccountView, PiggyBankView};

/// A new piggy bank. `currency` defaults to the household's primary
/// currency; every linked account must be an asset account in it.
#[derive(Debug, Clone, Default)]
pub struct NewPiggyBank {
    pub name: String,
    pub currency: Option<String>,
    /// Positive decimal string; `None` means no target.
    pub target_amount: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub target_date: Option<NaiveDate>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    pub notes: Option<String>,
    pub account_ids: Vec<AccountId>,
}

/// Changes to a piggy bank. `None` keeps a value; `Some(None)` clears a
/// nullable one. The currency never changes.
#[derive(Debug, Clone, Default)]
pub struct PiggyBankUpdate {
    pub name: Option<String>,
    pub target_amount: Option<Option<String>>,
    pub start_date: Option<Option<NaiveDate>>,
    pub target_date: Option<Option<NaiveDate>>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    pub notes: Option<Option<String>>,
    pub account_ids: Option<Vec<AccountId>>,
}

const DUPLICATE: &str = "a piggy bank with this name exists";
const ENTITY: &str = "piggy_bank";

#[derive(Clone)]
pub struct PiggyBankService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl PiggyBankService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }

    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewPiggyBank,
    ) -> Result<PiggyBankView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let result = self.create_in(&mut tx, ctx, origin, input).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::create`], inside the caller's transaction (used by imports).
    pub async fn create_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewPiggyBank,
    ) -> Result<PiggyBankView, AppError> {
        let code = match &input.currency {
            Some(code) => code.clone(),
            None => identity::primary_currency(&mut *conn, ctx).await?,
        };
        let currency = currencies::find(&mut *conn, &code, "currency").await?;
        let now = format_timestamp(self.clock.now());
        let row = PiggyBankRow {
            id: new_id(&*self.clock).to_string(),
            household_id: ctx.household_id.to_string(),
            name: identity::required("name", &input.name)?,
            target_amount_minor: parse_target(input.target_amount.as_deref(), &currency)?,
            currency_code: currency.code().to_string(),
            start_date: input.start_date.map(|d| d.to_string()),
            target_date: input.target_date.map(|d| d.to_string()),
            active: input.active.unwrap_or(true),
            sort_order: input.sort_order.unwrap_or(0),
            notes: input.notes,
            created_at: now.clone(),
            updated_at: now,
        };
        check_dates(&row)?;
        let accounts = check_accounts(&mut *conn, ctx, &currency, &input.account_ids).await?;
        repo::insert(&mut *conn, &row)
            .await
            .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        repo::set_accounts(&mut *conn, &row.household_id, &row.id, &accounts).await?;
        let view = view_of(&mut *conn, row).await?;
        let event = NewEvent::new(&*self.clock, ENTITY, "created", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: PiggyBankId,
        update: PiggyBankUpdate,
    ) -> Result<PiggyBankView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let result = self.update_in(&mut tx, ctx, origin, id, update).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::update`], inside the caller's transaction (used by imports).
    /// An account that still holds saved money cannot be unlinked.
    pub async fn update_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: PiggyBankId,
        update: PiggyBankUpdate,
    ) -> Result<PiggyBankView, AppError> {
        let current = find(&mut *conn, ctx, id).await?;
        let currency = currencies::find(&mut *conn, &current.currency_code, "currency").await?;
        let target_amount_minor = match &update.target_amount {
            Some(text) => parse_target(text.as_deref(), &currency)?,
            None => current.target_amount_minor,
        };
        let date_text = |d: Option<NaiveDate>| d.map(|d| d.to_string());
        let row = PiggyBankRow {
            name: match &update.name {
                Some(name) => identity::required("name", name)?,
                None => current.name.clone(),
            },
            target_amount_minor,
            start_date: update
                .start_date
                .map_or(current.start_date.clone(), date_text),
            target_date: update
                .target_date
                .map_or(current.target_date.clone(), date_text),
            active: update.active.unwrap_or(current.active),
            sort_order: update.sort_order.unwrap_or(current.sort_order),
            notes: update.notes.unwrap_or(current.notes.clone()),
            updated_at: format_timestamp(self.clock.now()),
            ..current.clone()
        };
        check_dates(&row)?;
        if let Some(ids) = &update.account_ids {
            let accounts = check_accounts(&mut *conn, ctx, &currency, ids).await?;
            check_unlinked_are_empty(&mut *conn, &current, &accounts).await?;
            repo::set_accounts(&mut *conn, &row.household_id, &row.id, &accounts).await?;
        }
        repo::update(&mut *conn, &row)
            .await
            .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = view_of(&mut *conn, row).await?;
        let event = NewEvent::new(&*self.clock, ENTITY, "updated", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }

    /// Deletes the piggy bank with its events. The money stays in the
    /// accounts; only the earmark disappears.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: PiggyBankId,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        self.delete_in(&mut tx, ctx, origin, id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Like [`Self::delete`], inside the caller's transaction.
    pub async fn delete_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: PiggyBankId,
    ) -> Result<(), AppError> {
        let row = find(&mut *conn, ctx, id).await?;
        let view = view_of(&mut *conn, row.clone()).await?;
        repo::delete(&mut *conn, &row.household_id, &row.id).await?;
        let event = NewEvent::new(&*self.clock, ENTITY, "deleted", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(())
    }

    pub async fn get(
        &self,
        ctx: &HouseholdCtx,
        id: PiggyBankId,
    ) -> Result<PiggyBankView, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.get_in(&mut conn, ctx, id).await
    }

    /// Like [`Self::get`], inside the caller's transaction.
    pub async fn get_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: PiggyBankId,
    ) -> Result<PiggyBankView, AppError> {
        let row = find(&mut *conn, ctx, id).await?;
        view_of(conn, row).await
    }

    /// Piggy banks ordered by sort order and name; `active` filters.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        active: Option<bool>,
        page: PageRequest,
    ) -> Result<Page<PiggyBankView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let household_id = ctx.household_id.to_string();
        let (rows, total) = repo::list(
            &mut conn,
            &household_id,
            active,
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
}

pub(crate) async fn find(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: PiggyBankId,
) -> Result<PiggyBankRow, AppError> {
    repo::get(conn, &ctx.household_id.to_string(), &id.to_string())
        .await?
        .ok_or(AppError::NotFound)
}

fn parse_target(text: Option<&str>, currency: &Currency) -> Result<Option<i64>, AppError> {
    let Some(text) = text else {
        return Ok(None);
    };
    let money = Money::parse_decimal(text, currency)
        .map_err(|e| AppError::invalid("target_amount", e.to_string()))?;
    if money.minor() <= 0 {
        return Err(AppError::invalid("target_amount", "must be positive"));
    }
    Ok(Some(money.minor()))
}

fn check_dates(row: &PiggyBankRow) -> Result<(), AppError> {
    // `if let (Some(a), Some(b)) = (..)` matches both options at once.
    if let (Some(start), Some(target)) = (&row.start_date, &row.target_date)
        && start > target
    {
        return Err(AppError::invalid(
            "target_date",
            "must not be before the start date",
        ));
    }
    Ok(())
}

/// The linked accounts, de-duplicated and sorted: at least one, each an
/// asset account of the household in the piggy bank's currency.
async fn check_accounts(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    currency: &Currency,
    ids: &[AccountId],
) -> Result<Vec<String>, AppError> {
    let ids: BTreeSet<String> = ids.iter().map(ToString::to_string).collect();
    if ids.is_empty() {
        return Err(AppError::invalid(
            "account_ids",
            "must name at least one account",
        ));
    }
    let household_id = ctx.household_id.to_string();
    for id in &ids {
        let field = "account_ids";
        let account = account_repo::get(&mut *conn, &household_id, id)
            .await?
            .ok_or_else(|| AppError::invalid(field, format!("unknown account {id}")))?;
        if account.account_type != "asset" {
            return Err(AppError::invalid(
                field,
                format!("{} is not an asset account", account.name),
            ));
        }
        if account.currency_code != currency.code().to_string() {
            return Err(AppError::invalid(
                field,
                format!("{} is not in {}", account.name, currency.code()),
            ));
        }
    }
    Ok(ids.into_iter().collect())
}

/// Refuses to unlink an account that still holds saved money.
async fn check_unlinked_are_empty(
    conn: &mut SqliteConnection,
    current: &PiggyBankRow,
    keep: &[String],
) -> Result<(), AppError> {
    let linked = repo::accounts(&mut *conn, &current.household_id, &current.id).await?;
    let still_saved = linked
        .iter()
        .find(|(id, saved)| *saved != 0 && !keep.contains(id));
    match still_saved {
        Some((id, _)) => Err(AppError::Conflict(format!(
            "money is still saved in account {id}; remove it first"
        ))),
        None => Ok(()),
    }
}
