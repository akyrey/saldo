//! Bills: recurring expected payments, their due periods, and reminders
//! (ARCHITECTURE.md 5.3 and 8.3).

mod input;
mod periods;
mod scheduler;
mod view;

use saldo_core::BillId;
use sqlx::{SqliteConnection, SqlitePool};

use crate::attachments::delete_all_of_in;
use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity::{required, today_in};
use crate::page::{Page, PageRequest};
use crate::{currencies, identity};

pub use input::{BillUpdate, NewBill};
use input::{
    DEFAULT_REMIND_DAYS_BEFORE, DEFAULT_REMIND_EVERY_DAYS, check_dates, check_every, parse_amounts,
    parse_frequency,
};
pub(crate) use periods::all_periods_in;
pub use periods::{OpenPeriod, existing, link_splits_in, release_group_in};
pub use scheduler::{REMINDER_INTERVAL, run_reminders};
pub use view::{BillPeriodView, BillView};
use view::{BillRow, view_of};

const DUPLICATE: &str = "a bill with this name exists";

#[derive(Clone)]
pub struct BillService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl BillService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }

    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewBill,
    ) -> Result<BillView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let view = self.create_in(&mut tx, ctx, origin, input).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Like [`Self::create`], inside the caller's transaction. Also creates
    /// the bill's periods up to 60 days ahead.
    pub async fn create_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewBill,
    ) -> Result<BillView, AppError> {
        let frequency = parse_frequency(&input.repeat_freq)?;
        check_dates(input.first_due_date, input.end_date)?;
        let every = input.remind_every_days.unwrap_or(DEFAULT_REMIND_EVERY_DAYS);
        check_every(every)?;
        let code = match &input.currency {
            Some(code) => code.clone(),
            None => identity::primary_currency(&mut *conn, ctx).await?,
        };
        let currency = currencies::find(&mut *conn, &code, "currency").await?;
        let (min, max) = parse_amounts(&input.amount_min, &input.amount_max, &currency)?;
        let now = format_timestamp(self.clock.now());
        let row = BillRow {
            id: new_id(&*self.clock).to_string(),
            household_id: ctx.household_id.to_string(),
            name: required("name", &input.name)?,
            amount_min_minor: min,
            amount_max_minor: max,
            currency_code: currency.code().to_string(),
            first_due_date: input.first_due_date.to_string(),
            end_date: input.end_date.map(|d| d.to_string()),
            repeat_freq: frequency.as_str().to_string(),
            skip: i64::from(input.skip.unwrap_or(0)),
            remind_days_before: i64::from(
                input
                    .remind_days_before
                    .unwrap_or(DEFAULT_REMIND_DAYS_BEFORE),
            ),
            remind_every_days: i64::from(every),
            active: input.active.unwrap_or(true),
            sort_order: input.sort_order.unwrap_or(0),
            notes: input.notes,
            created_at: now.clone(),
            updated_at: now,
        };
        insert(&mut *conn, &row).await?;
        let today = today_in(&mut *conn, ctx, &*self.clock).await?;
        periods::materialise_bill(&mut *conn, &*self.clock, &row, today).await?;
        let view = view_of(&mut *conn, row).await?;
        let event = NewEvent::new(&*self.clock, "bill", "created", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: BillId,
        update: BillUpdate,
    ) -> Result<BillView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let view = self.update_in(&mut tx, ctx, origin, id, update).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Like [`Self::update`], inside the caller's transaction. Open future
    /// periods that the new schedule no longer has are removed.
    pub async fn update_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: BillId,
        update: BillUpdate,
    ) -> Result<BillView, AppError> {
        let current = find(&mut *conn, ctx, id).await?;
        let currency = currencies::find(&mut *conn, &current.currency_code, "currency").await?;
        let min_text = update
            .amount_min
            .clone()
            .unwrap_or_else(|| minor_text(current.amount_min_minor, &currency));
        let max_text = update
            .amount_max
            .clone()
            .unwrap_or_else(|| minor_text(current.amount_max_minor, &currency));
        let (min, max) = parse_amounts(&min_text, &max_text, &currency)?;
        let frequency = match &update.repeat_freq {
            Some(text) => parse_frequency(text)?,
            None => parse_frequency(&current.repeat_freq)?,
        };
        let row = BillRow {
            name: match &update.name {
                Some(name) => required("name", name)?,
                None => current.name.clone(),
            },
            amount_min_minor: min,
            amount_max_minor: max,
            first_due_date: update
                .first_due_date
                .map_or(current.first_due_date.clone(), |d| d.to_string()),
            end_date: update
                .end_date
                .map_or(current.end_date.clone(), |d| d.map(|d| d.to_string())),
            repeat_freq: frequency.as_str().to_string(),
            skip: update.skip.map_or(current.skip, i64::from),
            remind_days_before: update
                .remind_days_before
                .map_or(current.remind_days_before, i64::from),
            remind_every_days: update
                .remind_every_days
                .map_or(current.remind_every_days, i64::from),
            active: update.active.unwrap_or(current.active),
            sort_order: update.sort_order.unwrap_or(current.sort_order),
            notes: update.notes.clone().unwrap_or(current.notes.clone()),
            updated_at: format_timestamp(self.clock.now()),
            ..current.clone()
        };
        let domain = row.domain()?;
        check_dates(domain.schedule.first_due_date, domain.schedule.end_date)?;
        check_every(domain.remind_every_days)?;
        write(&mut *conn, &row).await?;
        let today = today_in(&mut *conn, ctx, &*self.clock).await?;
        periods::drop_stale(&mut *conn, &row, today).await?;
        periods::materialise_bill(&mut *conn, &*self.clock, &row, today).await?;
        let view = view_of(&mut *conn, row).await?;
        let event = NewEvent::new(&*self.clock, "bill", "updated", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }

    /// Deletes the bill, its periods and its attachments; splits that
    /// referred to it keep no bill.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: BillId,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let row = find(&mut tx, ctx, id).await?;
        let view = view_of(&mut tx, row.clone()).await?;
        sqlx::query!(
            "UPDATE splits SET bill_id = NULL WHERE household_id = ? AND bill_id = ?",
            row.household_id,
            row.id
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "DELETE FROM bills WHERE household_id = ? AND id = ?",
            row.household_id,
            row.id
        )
        .execute(&mut *tx)
        .await?;
        let ids = [view.id.clone()];
        delete_all_of_in(&mut tx, &*self.clock, ctx, origin, "bill", &ids).await?;
        let event = NewEvent::new(&*self.clock, "bill", "deleted", view.id.clone(), &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: BillId) -> Result<BillView, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.get_in(&mut conn, ctx, id).await
    }

    pub async fn get_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: BillId,
    ) -> Result<BillView, AppError> {
        let row = find(&mut *conn, ctx, id).await?;
        view_of(conn, row).await
    }

    /// Bills ordered by sort order and name.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        page: PageRequest,
    ) -> Result<Page<BillView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.list_in(&mut conn, ctx, page).await
    }

    pub async fn list_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        page: PageRequest,
    ) -> Result<Page<BillView>, AppError> {
        let household_id = ctx.household_id.to_string();
        let items = bill_views(conn, &household_id, page.limit(), page.offset()).await?;
        let total = sqlx::query_scalar!(
            "SELECT count(*) FROM bills WHERE household_id = ?",
            household_id
        )
        .fetch_one(&mut *conn)
        .await?;
        Ok(Page::new(items, page, total))
    }
}

/// Every bill of the household, for the export.
pub(crate) async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<BillView>, AppError> {
    // SQLite reads `LIMIT -1` as "no limit".
    bill_views(conn, &ctx.household_id.to_string(), -1, 0).await
}

async fn bill_views(
    conn: &mut SqliteConnection,
    household_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<BillView>, AppError> {
    let rows = sqlx::query_as!(
            BillRow,
            r#"SELECT id, household_id, name, amount_min_minor, amount_max_minor, currency_code, first_due_date,
                      end_date, repeat_freq, skip, remind_days_before, remind_every_days,
                      active AS "active: bool", sort_order, notes, created_at, updated_at
               FROM bills WHERE household_id = ? ORDER BY sort_order, name LIMIT ? OFFSET ?"#,
            household_id,
            limit,
            offset,
        )
    .fetch_all(&mut *conn)
    .await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(view_of(&mut *conn, row).await?);
    }
    Ok(items)
}

/// An amount in minor units back as a decimal string in its currency.
fn minor_text(minor: i64, currency: &saldo_core::Currency) -> String {
    saldo_core::Money::new(minor, currency.code()).to_decimal_string(currency)
}

pub(crate) async fn find(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: BillId,
) -> Result<BillRow, AppError> {
    let (household_id, id) = (ctx.household_id.to_string(), id.to_string());
    sqlx::query_as!(
        BillRow,
        r#"SELECT id, household_id, name, amount_min_minor, amount_max_minor, currency_code, first_due_date,
                  end_date, repeat_freq, skip, remind_days_before, remind_every_days,
                  active AS "active: bool", sort_order, notes, created_at, updated_at
           FROM bills WHERE household_id = ? AND id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

async fn insert(conn: &mut SqliteConnection, row: &BillRow) -> Result<(), AppError> {
    sqlx::query!(
        "INSERT INTO bills (id, household_id, name, amount_min_minor, amount_max_minor, currency_code,
            first_due_date, end_date, repeat_freq, skip, remind_days_before, remind_every_days, active,
            sort_order, notes, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        row.id, row.household_id, row.name, row.amount_min_minor, row.amount_max_minor, row.currency_code,
        row.first_due_date, row.end_date, row.repeat_freq, row.skip, row.remind_days_before, row.remind_every_days,
        row.active, row.sort_order, row.notes, row.created_at, row.updated_at,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
    Ok(())
}

async fn write(conn: &mut SqliteConnection, row: &BillRow) -> Result<(), AppError> {
    sqlx::query!(
        "UPDATE bills SET name = ?, amount_min_minor = ?, amount_max_minor = ?, first_due_date = ?, end_date = ?,
            repeat_freq = ?, skip = ?, remind_days_before = ?, remind_every_days = ?, active = ?, sort_order = ?,
            notes = ?, updated_at = ?
         WHERE household_id = ? AND id = ?",
        row.name, row.amount_min_minor, row.amount_max_minor, row.first_due_date, row.end_date,
        row.repeat_freq, row.skip, row.remind_days_before, row.remind_every_days, row.active, row.sort_order,
        row.notes, row.updated_at, row.household_id, row.id,
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
    Ok(())
}
