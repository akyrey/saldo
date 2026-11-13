//! Firing a recurrence: one transaction group per date, created from the
//! template through the transaction service.

use chrono::{Days, NaiveDate};
use saldo_core::RecurrenceId;
use saldo_core::recurrence::occurrences;
use serde::Serialize;
use serde_json::Map;
use sqlx::SqliteConnection;

use super::input::{ScheduleInput, schedule_of, stored_schedule};
use super::repo::{self, RecurrenceRow, TemplateSplitRow};
use super::view::decimal;
use super::{ENTITY, RecurrenceService, find};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::identity::today_in;
use crate::piggy_banks::add_for_split_in;
use crate::transactions::recurring::Firing;
use crate::transactions::{SplitInput, TransactionGroupView, TransactionInput};

/// The result of firing, also the data of the `recurrence.fired` event.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct RecurrenceFiredView {
    pub recurrence_id: String,
    pub date: String,
    /// False when the recurrence had already fired on this date.
    pub created: bool,
    pub group: TransactionGroupView,
}

/// What one run of [`RecurrenceService::fire_due`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FireDueReport {
    pub fired: Vec<RecurrenceFiredView>,
    /// Dates that could not fire (for example an account was archived); the
    /// runner tries them again next time.
    pub failures: Vec<FireFailure>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FireFailure {
    pub recurrence_id: String,
    pub date: NaiveDate,
    pub message: String,
}

/// How far ahead a preview looks for dates.
const PREVIEW_YEARS: u32 = 5;
/// The most dates a preview returns.
pub const MAX_PREVIEW: usize = 100;

impl RecurrenceService {
    /// Creates the group for `date`, or returns the one created earlier: the
    /// UNIQUE `(recurrence_id, recurrence_date)` makes this idempotent.
    /// `date` need not be a scheduled date (a manual "fire now").
    pub async fn fire(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: RecurrenceId,
        date: NaiveDate,
    ) -> Result<RecurrenceFiredView, AppError> {
        let mut tx = self.pool.begin().await?;
        let result = self.fire_in(&mut tx, ctx, origin, id, date).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::fire`], inside the caller's transaction.
    pub async fn fire_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: RecurrenceId,
        date: NaiveDate,
    ) -> Result<RecurrenceFiredView, AppError> {
        let row = find(&mut *conn, ctx, id).await?;
        let templates = repo::splits(&mut *conn, &row.household_id, &row.id).await?;
        let input = group_input(&mut *conn, &row, &templates, date).await?;
        let (group, created) = self
            .transactions
            .create_for_recurrence_in(
                &mut *conn,
                ctx,
                origin,
                input,
                Firing {
                    recurrence_id: &row.id,
                    date,
                    apply_rules: row.apply_rules,
                },
            )
            .await?;
        let view = RecurrenceFiredView {
            recurrence_id: row.id.clone(),
            date: date.to_string(),
            created,
            group,
        };
        if !created {
            return Ok(view);
        }
        for (template, split) in templates.iter().zip(&view.group.splits) {
            if let Some(piggy) = &template.piggy_bank_id {
                let piggy = parse_stored(piggy)?;
                add_for_split_in(&mut *conn, &*self.clock, ctx, origin, piggy, split).await?;
            }
        }
        let event = NewEvent::new(&*self.clock, ENTITY, "fired", row.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }

    /// Fires every active recurrence for each date that is due up to the
    /// household's today and was not handled yet, each date in its own
    /// transaction with `Origin::Scheduler`. Item failures are reported, not
    /// returned; a database error stops the run.
    pub async fn fire_due(&self, ctx: &HouseholdCtx) -> Result<FireDueReport, AppError> {
        let mut conn = self.pool.acquire().await?;
        let today = today_in(&mut conn, ctx, &*self.clock).await?;
        let ids = repo::all_ids(&mut conn, &ctx.household_id.to_string()).await?;
        drop(conn);
        let mut report = FireDueReport::default();
        for id in ids {
            self.fire_due_one(ctx, parse_stored(&id)?, today, &mut report)
                .await?;
        }
        Ok(report)
    }

    async fn fire_due_one(
        &self,
        ctx: &HouseholdCtx,
        id: RecurrenceId,
        today: NaiveDate,
        report: &mut FireDueReport,
    ) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        let row = find(&mut conn, ctx, id).await?;
        let dates = if row.active {
            due_dates(&mut conn, &row, today).await?
        } else {
            // Inactive: nothing fires, and reactivating does not catch up.
            Vec::new()
        };
        drop(conn);
        let mut handled = today;
        for date in dates {
            match self.fire(ctx, Origin::Scheduler, id, date).await {
                Ok(view) => report.fired.push(view),
                Err(AppError::Database(error)) => return Err(AppError::Database(error)),
                Err(error) => {
                    report.failures.push(FireFailure {
                        recurrence_id: row.id.clone(),
                        date,
                        message: error.to_string(),
                    });
                    handled = date.pred_opt().unwrap_or(date);
                    break;
                }
            }
        }
        let mut conn = self.pool.acquire().await?;
        self.mark_fired_through_in(&mut conn, ctx, id, handled)
            .await
    }

    /// The first `count` dates of a schedule on or after the household's
    /// today (at most [`MAX_PREVIEW`], looking five years ahead).
    pub async fn preview(
        &self,
        ctx: &HouseholdCtx,
        input: &ScheduleInput,
        count: usize,
    ) -> Result<Vec<NaiveDate>, AppError> {
        let schedule = schedule_of(input)?;
        let mut conn = self.pool.acquire().await?;
        let today = today_in(&mut conn, ctx, &*self.clock).await?;
        let from = today.max(schedule.first_date);
        let to = from
            .checked_add_months(chrono::Months::new(12 * PREVIEW_YEARS))
            .unwrap_or(NaiveDate::MAX);
        let mut dates = occurrences(&schedule, from, to);
        dates.truncate(count.min(MAX_PREVIEW));
        Ok(dates)
    }
}

/// Dates after the runner's progress (or from the first date) up to today.
async fn due_dates(
    conn: &mut SqliteConnection,
    row: &RecurrenceRow,
    today: NaiveDate,
) -> Result<Vec<NaiveDate>, AppError> {
    let repetitions = repo::repetitions(&mut *conn, &row.household_id, &row.id).await?;
    let schedule = stored_schedule(row, &repetitions)?;
    let from = match row.fired_through.as_deref() {
        Some(done) => parse_stored::<NaiveDate>(done)?
            .checked_add_days(Days::new(1))
            .unwrap_or(NaiveDate::MAX),
        None => schedule.first_date,
    };
    Ok(occurrences(&schedule, from, today))
}

/// The group a firing on `date` creates: the template splits, dated `date`,
/// titled after the recurrence.
async fn group_input(
    conn: &mut SqliteConnection,
    row: &RecurrenceRow,
    templates: &[TemplateSplitRow],
    date: NaiveDate,
) -> Result<TransactionInput, AppError> {
    let mut splits = Vec::with_capacity(templates.len());
    for t in templates {
        let foreign_amount = match (t.foreign_amount_minor, &t.foreign_currency_code) {
            (Some(minor), Some(code)) => Some(decimal(&mut *conn, minor, code).await?),
            _ => None,
        };
        let tags: Vec<String> = serde_json::from_str(&t.tags)
            .map_err(|e| AppError::Internal(format!("corrupt template tags: {e}")))?;
        splits.push(SplitInput {
            date,
            description: t.description.clone(),
            source_id: parse_stored(&t.source_account_id)?,
            destination_id: parse_stored(&t.destination_account_id)?,
            amount: decimal(&mut *conn, t.amount_minor, &t.currency_code).await?,
            currency: Some(t.currency_code.clone()),
            foreign_amount,
            foreign_currency: t.foreign_currency_code.clone(),
            category_id: t.category_id.as_deref().map(parse_stored).transpose()?,
            budget_id: t.budget_id.as_deref().map(parse_stored).transpose()?,
            bill_id: t.bill_id.as_deref().map(parse_stored).transpose()?,
            tag_ids: tags
                .iter()
                .map(|t| parse_stored(t))
                .collect::<Result<_, _>>()?,
            reconciled: false,
            notes: None,
            meta: Map::new(),
        });
    }
    Ok(TransactionInput {
        title: Some(row.title.clone()),
        splits,
    })
}
