//! Accounts: create, change, delete, list, and compute balances.

mod balances;
mod input;
pub(crate) mod repo;
mod view;

use chrono::NaiveDate;
use saldo_core::AccountId;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::page::{Page, PageRequest};
use crate::scope::Scope;
use crate::{currencies, identity};

pub(crate) use input::core_account;
use input::{ALL_TYPES, CREATABLE_TYPES, apply_update, check_row, parse_virtual_balance};
pub use input::{AccountUpdate, NewAccount};
use repo::AccountRow;
pub use view::AccountView;
pub(crate) use view::view_of;

/// Filters for listing accounts.
#[derive(Debug, Clone, Default)]
pub struct AccountFilter {
    pub account_type: Option<String>,
    pub active: Option<bool>,
    pub scope: Scope,
    /// Part of the name, ignoring ASCII case; blank means "any".
    pub q: Option<String>,
}

/// An account's balance on a date (all time when `date` is `None`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BalanceView {
    pub account_id: String,
    pub date: Option<String>,
    pub balance: String,
    pub currency: String,
}

#[derive(Clone)]
pub struct AccountService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl AccountService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }

    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewAccount,
    ) -> Result<AccountView, AppError> {
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
        input: NewAccount,
    ) -> Result<AccountView, AppError> {
        if !CREATABLE_TYPES.contains(&input.account_type.as_str()) {
            return Err(AppError::invalid(
                "type",
                format!("must be one of {}", CREATABLE_TYPES.join(", ")),
            ));
        }
        let currency_code = match &input.currency {
            Some(code) => currencies::find(&mut *conn, code, "currency")
                .await?
                .code()
                .to_string(),
            None => identity::primary_currency(&mut *conn, ctx).await?,
        };
        let virtual_balance_minor = match &input.virtual_balance {
            Some(text) => parse_virtual_balance(&mut *conn, &currency_code, text).await?,
            None => 0,
        };
        let now = format_timestamp(self.clock.now());
        let row = AccountRow {
            id: new_id(&*self.clock).to_string(),
            household_id: ctx.household_id.to_string(),
            owner_user_id: input.owner_user_id.map(|u| u.to_string()),
            name: identity::required("name", &input.name)?,
            account_type: input.account_type,
            equity_role: None,
            asset_role: input.asset_role,
            liability_type: input.liability_type,
            liability_direction: input.liability_direction,
            interest_rate: input.interest_rate,
            interest_period: input.interest_period,
            currency_code,
            iban: input.iban,
            bic: input.bic,
            account_number: input.account_number,
            virtual_balance_minor,
            include_net_worth: input.include_net_worth.unwrap_or(true),
            active: input.active.unwrap_or(true),
            sort_order: input.sort_order.unwrap_or(0),
            notes: input.notes,
            created_at: now.clone(),
            updated_at: now,
        };
        check_row(&mut *conn, ctx, &row).await?;
        repo::insert(&mut *conn, &row).await?;
        let view = view_of(&mut *conn, row).await?;
        let event = NewEvent::new(&*self.clock, "account", "created", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: AccountId,
        update: AccountUpdate,
    ) -> Result<AccountView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let result = self.update_in(&mut tx, ctx, origin, id, update).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::update`], inside the caller's transaction (used by imports).
    pub async fn update_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: AccountId,
        update: AccountUpdate,
    ) -> Result<AccountView, AppError> {
        let current = self.find(&mut *conn, ctx, id).await?;
        let virtual_balance = update.virtual_balance.clone();
        let mut row = apply_update(&current, update)?;
        row.name = identity::required("name", &row.name)?;
        if let Some(text) = virtual_balance {
            row.virtual_balance_minor =
                parse_virtual_balance(&mut *conn, &row.currency_code, &text).await?;
        }
        row.updated_at = format_timestamp(self.clock.now());
        check_row(&mut *conn, ctx, &row).await?;
        repo::update(&mut *conn, &row).await?;
        let view = view_of(&mut *conn, row).await?;
        let event = NewEvent::new(&*self.clock, "account", "updated", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }

    /// Deletes an account that has no transactions. System equity accounts
    /// cannot be deleted.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: AccountId,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        self.delete_in(&mut tx, ctx, origin, id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Like [`Self::delete`], inside the caller's transaction (used by imports).
    pub async fn delete_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: AccountId,
    ) -> Result<(), AppError> {
        let row = self.find(&mut *conn, ctx, id).await?;
        if row.account_type == "equity" {
            return Err(AppError::Conflict(
                "system equity accounts cannot be deleted".to_string(),
            ));
        }
        if repo::has_splits(&mut *conn, &row.household_id, &row.id).await? {
            return Err(AppError::Conflict(
                "the account has transactions".to_string(),
            ));
        }
        repo::delete(&mut *conn, &row.household_id, &row.id).await?;
        let view = view_of(&mut *conn, row).await?;
        let event = NewEvent::new(&*self.clock, "account", "deleted", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(())
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: AccountId) -> Result<AccountView, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.get_in(&mut conn, ctx, id).await
    }

    /// Like [`Self::get`], inside the caller's transaction.
    pub async fn get_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: AccountId,
    ) -> Result<AccountView, AppError> {
        let row = self.find(&mut *conn, ctx, id).await?;
        view_of(conn, row).await
    }

    /// Accounts ordered by type, sort order and name.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        filter: &AccountFilter,
        page: PageRequest,
    ) -> Result<Page<AccountView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.list_in(&mut conn, ctx, filter, page).await
    }

    /// Like [`Self::list`], inside the caller's transaction.
    pub async fn list_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        filter: &AccountFilter,
        page: PageRequest,
    ) -> Result<Page<AccountView>, AppError> {
        let (rows, total) = list_rows(&mut *conn, ctx, filter, page).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(view_of(&mut *conn, row).await?);
        }
        Ok(Page::new(items, page, total))
    }

    /// The balance up to and including `date`, or of all time.
    pub async fn balance(
        &self,
        ctx: &HouseholdCtx,
        id: AccountId,
        date: Option<NaiveDate>,
    ) -> Result<BalanceView, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.balance_in(&mut conn, ctx, id, date).await
    }

    /// Like [`Self::balance`], inside the caller's transaction.
    pub async fn balance_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: AccountId,
        date: Option<NaiveDate>,
    ) -> Result<BalanceView, AppError> {
        let row = self.find(&mut *conn, ctx, id).await?;
        let date = date.map(|d| d.to_string());
        balances::balance_view(&mut *conn, row, date.as_deref()).await
    }

    async fn find(
        &self,
        conn: &mut sqlx::SqliteConnection,
        ctx: &HouseholdCtx,
        id: AccountId,
    ) -> Result<AccountRow, AppError> {
        repo::get(conn, &ctx.household_id.to_string(), &id.to_string())
            .await?
            .ok_or(AppError::NotFound)
    }
}

/// One page of account rows matching `filter`, plus the total. Shared by
/// [`AccountService::list`] and [`AccountService::balances`].
async fn list_rows(
    conn: &mut sqlx::SqliteConnection,
    ctx: &HouseholdCtx,
    filter: &AccountFilter,
    page: PageRequest,
) -> Result<(Vec<AccountRow>, i64), AppError> {
    let unknown_type = |t: &String| !ALL_TYPES.contains(&t.as_str());
    if filter.account_type.as_ref().is_some_and(unknown_type) {
        return Err(AppError::invalid(
            "type",
            format!("must be one of {}", ALL_TYPES.join(", ")),
        ));
    }
    let repo_filter = repo::ListFilter {
        account_type: filter.account_type.clone(),
        active: filter.active,
        owner_user_id: filter.scope.owner(ctx),
        q: filter.q.clone().filter(|q| !q.trim().is_empty()),
        limit: page.limit(),
        offset: page.offset(),
    };
    repo::list(conn, &ctx.household_id.to_string(), &repo_filter).await
}
