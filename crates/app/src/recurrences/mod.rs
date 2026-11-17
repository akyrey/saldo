//! Recurring transactions: a template (splits) plus a schedule
//! (repetitions). Firing one creates a transaction group (see `fire.rs`).

mod fire;
mod input;
mod repo;
pub(crate) mod upcoming;
mod view;

use saldo_core::RecurrenceId;
use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::page::{Page, PageRequest};
use crate::transactions::TransactionService;

pub use fire::{FireDueReport, FireFailure, RecurrenceFiredView};
pub use input::{RecurrenceInput, RepetitionInput, ScheduleInput, TemplateSplitInput};
use repo::RecurrenceRow;
use view::view_of;
pub use view::{RecurrenceView, RepetitionView, TemplateSplitView};

const ENTITY: &str = "recurrence";

#[derive(Clone)]
pub struct RecurrenceService {
    pool: SqlitePool,
    clock: SharedClock,
    /// Firing goes through the transaction service, so a recurring group
    /// passes the same validation and writes the same event as any other.
    transactions: TransactionService,
}

impl RecurrenceService {
    pub fn new(pool: SqlitePool, clock: SharedClock, transactions: TransactionService) -> Self {
        Self {
            pool,
            clock,
            transactions,
        }
    }

    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: RecurrenceInput,
    ) -> Result<RecurrenceView, AppError> {
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
        input: RecurrenceInput,
    ) -> Result<RecurrenceView, AppError> {
        let now = format_timestamp(self.clock.now());
        let blank = RecurrenceRow {
            id: new_id(&*self.clock).to_string(),
            household_id: ctx.household_id.to_string(),
            title: String::new(),
            description: None,
            first_date: String::new(),
            repeat_until: None,
            max_occurrences: None,
            apply_rules: true,
            active: true,
            notes: None,
            fired_through: None,
            created_at: now.clone(),
            updated_at: now,
        };
        let prepared = input::prepare(&mut *conn, &*self.clock, ctx, origin, blank, input).await?;
        let row = prepared.row;
        repo::insert(&mut *conn, &row).await?;
        repo::replace_children(
            &mut *conn,
            &row.household_id,
            &row.id,
            &prepared.repetitions,
            &prepared.splits,
        )
        .await?;
        let view = view_of(&mut *conn, row).await?;
        let event = NewEvent::new(&*self.clock, ENTITY, "created", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }

    /// Replaces everything but the ID and the runner's progress.
    pub async fn replace(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: RecurrenceId,
        input: RecurrenceInput,
    ) -> Result<RecurrenceView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let result = self.replace_in(&mut tx, ctx, origin, id, input).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::replace`], inside the caller's transaction (used by imports).
    pub async fn replace_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: RecurrenceId,
        input: RecurrenceInput,
    ) -> Result<RecurrenceView, AppError> {
        let current = find(&mut *conn, ctx, id).await?;
        let prepared =
            input::prepare(&mut *conn, &*self.clock, ctx, origin, current, input).await?;
        let row = RecurrenceRow {
            updated_at: format_timestamp(self.clock.now()),
            ..prepared.row
        };
        repo::update(&mut *conn, &row).await?;
        repo::replace_children(
            &mut *conn,
            &row.household_id,
            &row.id,
            &prepared.repetitions,
            &prepared.splits,
        )
        .await?;
        let view = view_of(&mut *conn, row).await?;
        let event = NewEvent::new(&*self.clock, ENTITY, "updated", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }

    /// Deletes the recurrence. Transactions it created stay.
    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: RecurrenceId,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let row = find(&mut tx, ctx, id).await?;
        let view = view_of(&mut tx, row.clone()).await?;
        repo::delete(&mut tx, &row.household_id, &row.id).await?;
        let event = NewEvent::new(&*self.clock, ENTITY, "deleted", view.id.clone(), &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(
        &self,
        ctx: &HouseholdCtx,
        id: RecurrenceId,
    ) -> Result<RecurrenceView, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.get_in(&mut conn, ctx, id).await
    }

    /// Like [`Self::get`], inside the caller's transaction.
    pub async fn get_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: RecurrenceId,
    ) -> Result<RecurrenceView, AppError> {
        let row = find(&mut *conn, ctx, id).await?;
        view_of(conn, row).await
    }

    /// Recurrences ordered by title; `active` filters.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        active: Option<bool>,
        page: PageRequest,
    ) -> Result<Page<RecurrenceView>, AppError> {
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

    /// Marks every date up to `date` as handled, so the runner never fires
    /// them (the importer uses this: Firefly already created those
    /// transactions). Never moves backwards.
    pub async fn mark_fired_through_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: RecurrenceId,
        date: chrono::NaiveDate,
    ) -> Result<(), AppError> {
        let row = find(&mut *conn, ctx, id).await?;
        let date = date.to_string();
        if row
            .fired_through
            .as_deref()
            .is_some_and(|current| current >= date.as_str())
        {
            return Ok(());
        }
        repo::set_fired_through(&mut *conn, &row.household_id, &row.id, &date).await
    }
}

pub(crate) async fn find(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: RecurrenceId,
) -> Result<RecurrenceRow, AppError> {
    repo::get(conn, &ctx.household_id.to_string(), &id.to_string())
        .await?
        .ok_or(AppError::NotFound)
}

/// Every recurrence of the household, for the export.
pub(crate) async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<RecurrenceView>, AppError> {
    let household_id = ctx.household_id.to_string();
    // SQLite reads `LIMIT -1` as "no limit".
    let (rows, _) = repo::list(&mut *conn, &household_id, None, -1, 0).await?;
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        views.push(view_of(&mut *conn, row).await?);
    }
    Ok(views)
}
