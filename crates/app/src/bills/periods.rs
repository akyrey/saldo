//! Bill periods: creating them ahead of time, paying, skipping, reopening,
//! reminders, and the links from transactions (ARCHITECTURE.md 8.3).

use chrono::{Days, NaiveDate};
use saldo_core::{Bill, BillId, BillPeriod, BillPeriodId, ReminderKind, SplitId, reminder_due};
use serde::Serialize;
use sqlx::SqliteConnection;

use super::view::{BillPeriodView, BillRow, BillView, PeriodRow, view_of};
use super::{BillService, find};
use crate::clock::{Clock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::identity::today_in;
use crate::transactions::TransactionGroupView;

/// Periods are created this many days ahead of "today".
pub const HORIZON_DAYS: u64 = 60;

/// An open period with its bill, for the reminder scheduler (Phase 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPeriod {
    pub bill_id: BillId,
    pub period_id: BillPeriodId,
    pub bill: Bill,
    pub period: BillPeriod,
}

/// The event data of period changes: the bill and the period.
#[derive(Debug, Serialize)]
struct PeriodEvent {
    bill: BillView,
    period: BillPeriodView,
}

/// Creates the bill's missing periods up to `today + HORIZON_DAYS`. Safe to
/// repeat: `(bill_id, due_date)` is unique.
pub(crate) async fn materialise_bill(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    row: &BillRow,
    today: NaiveDate,
) -> Result<(), AppError> {
    if !row.active {
        return Ok(());
    }
    let schedule = row.domain()?.schedule;
    let latest = sqlx::query_scalar!(
        "SELECT max(due_date) FROM bill_periods WHERE bill_id = ?",
        row.id
    )
    .fetch_one(&mut *conn)
    .await?;
    // Start after the latest existing period instead of at the first due date.
    let from = match latest {
        Some(text) => parse_stored::<NaiveDate>(&text)? + Days::new(1),
        None => schedule.first_due_date,
    };
    let to = today + Days::new(HORIZON_DAYS);
    let now = format_timestamp(clock.now());
    for due in schedule.occurrences(from, to) {
        let (id, due) = (new_id(clock).to_string(), due.to_string());
        sqlx::query!(
            "INSERT INTO bill_periods (id, household_id, bill_id, due_date, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (bill_id, due_date) DO NOTHING",
            id,
            row.household_id,
            row.id,
            due,
            now,
            now,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// After a schedule change: removes open, never-reminded periods from today
/// on, so they are recreated on the new schedule.
pub(crate) async fn drop_stale(
    conn: &mut SqliteConnection,
    row: &BillRow,
    today: NaiveDate,
) -> Result<(), AppError> {
    let today = today.to_string();
    sqlx::query!(
        "DELETE FROM bill_periods WHERE bill_id = ? AND status = 'open' AND reminder_count = 0 AND due_date >= ?",
        row.id,
        today,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

impl BillService {
    /// Creates every active bill's periods up to 60 days after `today`. The
    /// Phase 6 scheduler calls this; bill create/update call it too.
    pub async fn materialise_periods(
        &self,
        ctx: &HouseholdCtx,
        today: NaiveDate,
    ) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
        for row in all_rows(&mut tx, ctx).await? {
            materialise_bill(&mut tx, &*self.clock, &row, today).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Every open period of the household's active bills, oldest first.
    pub async fn open_periods(&self, ctx: &HouseholdCtx) -> Result<Vec<OpenPeriod>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let mut open = Vec::new();
        for row in all_rows(&mut conn, ctx)
            .await?
            .into_iter()
            .filter(|r| r.active)
        {
            let bill = row.domain()?;
            for period in period_rows(&mut conn, &row.id, Some("open")).await? {
                open.push(OpenPeriod {
                    bill_id: parse_stored(&row.id)?,
                    period_id: parse_stored(&period.id)?,
                    bill,
                    period: period.domain()?,
                });
            }
        }
        open.sort_by_key(|p| p.period.due_date);
        Ok(open)
    }

    /// Records a reminder sent today: updates the period's reminder state and
    /// appends `bill.due_soon` or `bill.overdue`, in one transaction. Does
    /// nothing if a reminder was already recorded today (safe on restart).
    /// Returns whether a reminder was recorded.
    pub async fn record_reminder(
        &self,
        ctx: &HouseholdCtx,
        period: BillPeriodId,
        kind: ReminderKind,
        today: NaiveDate,
    ) -> Result<bool, AppError> {
        let mut tx = self.pool.begin().await?;
        let row = period_row(&mut tx, ctx, period).await?;
        let (day, now) = (today.to_string(), format_timestamp(self.clock.now()));
        let changed = sqlx::query!(
            "UPDATE bill_periods SET last_reminded_on = ?, reminder_count = reminder_count + 1, updated_at = ?
             WHERE id = ? AND status = 'open' AND (last_reminded_on IS NULL OR last_reminded_on < ?)",
            day, now, row.id, day,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed == 0 {
            return Ok(false);
        }
        emit(
            &mut tx,
            &*self.clock,
            ctx,
            Origin::Scheduler,
            &row.id,
            kind.event_type(),
        )
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Whether to remind about a period today; `core::reminder_due` on stored data.
    pub fn reminder_due(open: &OpenPeriod, today: NaiveDate) -> Option<ReminderKind> {
        reminder_due(&open.period, &open.bill, today)
    }

    /// A bill's periods, newest first.
    pub async fn periods(
        &self,
        ctx: &HouseholdCtx,
        bill: BillId,
    ) -> Result<Vec<BillPeriodView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let row = find(&mut conn, ctx, bill).await?;
        let mut rows = period_rows(&mut conn, &row.id, None).await?;
        rows.reverse();
        Ok(rows)
    }

    /// Marks a period paid by a split of the household (`bill.paid`).
    pub async fn pay(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        period: BillPeriodId,
        split: SplitId,
    ) -> Result<BillPeriodView, AppError> {
        let mut tx = self.pool.begin().await?;
        let row = period_row(&mut tx, ctx, period).await?;
        if row.status == "paid" {
            return Err(AppError::Conflict(
                "this period is already paid".to_string(),
            ));
        }
        let (household_id, split_id) = (ctx.household_id.to_string(), split.to_string());
        let exists = sqlx::query_scalar!(
            "SELECT EXISTS (SELECT 1 FROM splits WHERE household_id = ? AND id = ?)",
            household_id,
            split_id
        )
        .fetch_one(&mut *tx)
        .await?;
        if exists == 0 {
            return Err(AppError::invalid("split_id", "no split with this ID"));
        }
        pay_period(&mut tx, &*self.clock, ctx, origin, &row.id, &split_id).await?;
        let view = period_row(&mut tx, ctx, period).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Marks an open period as not needing payment.
    pub async fn skip(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        period: BillPeriodId,
    ) -> Result<BillPeriodView, AppError> {
        self.set_status(ctx, origin, period, "skipped", "period_skipped")
            .await
    }

    /// Opens a paid or skipped period again (`bill.period_reopened`).
    pub async fn reopen(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        period: BillPeriodId,
    ) -> Result<BillPeriodView, AppError> {
        self.set_status(ctx, origin, period, "open", "period_reopened")
            .await
    }

    async fn set_status(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        period: BillPeriodId,
        status: &str,
        verb: &str,
    ) -> Result<BillPeriodView, AppError> {
        let mut tx = self.pool.begin().await?;
        let row = period_row(&mut tx, ctx, period).await?;
        if row.status == status {
            return Err(AppError::Conflict(format!(
                "this period is already {status}"
            )));
        }
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "UPDATE bill_periods SET status = ?, paid_split_id = NULL, paid_at = NULL, updated_at = ? WHERE id = ?",
            status, now, row.id,
        )
        .execute(&mut *tx)
        .await?;
        emit(&mut tx, &*self.clock, ctx, origin, &row.id, verb).await?;
        let view = period_row(&mut tx, ctx, period).await?;
        tx.commit().await?;
        Ok(view)
    }
}

/// After a transaction is saved: each split with a bill pays the matching
/// open period of that bill (ARCHITECTURE.md 8.3).
pub async fn link_splits_in(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    ctx: &HouseholdCtx,
    origin: Origin,
    group: &TransactionGroupView,
) -> Result<(), AppError> {
    for split in &group.splits {
        let Some(bill_id) = split.bill_id.as_deref() else {
            continue;
        };
        let row = find(&mut *conn, ctx, parse_stored(bill_id)?).await?;
        if !row.active {
            continue;
        }
        let today = today_in(&mut *conn, ctx, clock).await?;
        materialise_bill(&mut *conn, clock, &row, today).await?;
        let open = period_rows(&mut *conn, &row.id, Some("open")).await?;
        let dues: Vec<NaiveDate> = open
            .iter()
            .map(|p| parse_stored(&p.due_date))
            .collect::<Result<_, _>>()?;
        let date: NaiveDate = parse_stored(&split.date)?;
        let Some(due) = row.domain()?.schedule.matching_due_date(&dues, date) else {
            continue;
        };
        if let Some(period) = open.iter().find(|p| p.due_date == due.to_string()) {
            pay_period(&mut *conn, clock, ctx, origin, &period.id, &split.id).await?;
        }
    }
    Ok(())
}

/// Before a group's splits are deleted (delete or replace): every period
/// they paid is open again (`bill.period_reopened`).
pub async fn release_group_in(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    ctx: &HouseholdCtx,
    origin: Origin,
    group_id: &str,
) -> Result<(), AppError> {
    let household_id = ctx.household_id.to_string();
    let paid = sqlx::query_scalar!(
        "SELECT p.id FROM bill_periods p JOIN splits s ON s.id = p.paid_split_id
         WHERE s.household_id = ? AND s.group_id = ?",
        household_id,
        group_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let now = format_timestamp(clock.now());
    for id in paid {
        sqlx::query!(
            "UPDATE bill_periods SET status = 'open', paid_split_id = NULL, paid_at = NULL, updated_at = ? WHERE id = ?",
            now, id,
        )
        .execute(&mut *conn)
        .await?;
        emit(&mut *conn, clock, ctx, origin, &id, "period_reopened").await?;
    }
    Ok(())
}

/// The bill IDs among `ids_json` (a JSON array) that exist in the household.
pub async fn existing(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    ids_json: &str,
) -> Result<Vec<String>, AppError> {
    let household_id = ctx.household_id.to_string();
    let ids = sqlx::query_scalar!(
        "SELECT id FROM bills WHERE household_id = ? AND id IN (SELECT value FROM json_each(?))",
        household_id,
        ids_json,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids)
}

async fn pay_period(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    ctx: &HouseholdCtx,
    origin: Origin,
    period_id: &str,
    split_id: &str,
) -> Result<(), AppError> {
    let now = format_timestamp(clock.now());
    sqlx::query!(
        "UPDATE bill_periods SET status = 'paid', paid_split_id = ?, paid_at = ?, updated_at = ? WHERE id = ?",
        split_id, now, now, period_id,
    )
    .execute(&mut *conn)
    .await?;
    emit(conn, clock, ctx, origin, period_id, "paid").await
}

/// Appends `bill.<verb>` with the bill and the period as data.
async fn emit(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    ctx: &HouseholdCtx,
    origin: Origin,
    period_id: &str,
    verb: &str,
) -> Result<(), AppError> {
    let period = period_row(&mut *conn, ctx, parse_stored(period_id)?).await?;
    let bill_row = find(&mut *conn, ctx, parse_stored(&period.bill_id)?).await?;
    let bill = view_of(&mut *conn, bill_row).await?;
    let data = PeriodEvent { bill, period };
    let event = NewEvent::new(clock, "bill", verb, data.bill.id.clone(), &data)?;
    append_event(conn, ctx, origin, event).await?;
    Ok(())
}

async fn all_rows(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<BillRow>, AppError> {
    let household_id = ctx.household_id.to_string();
    let rows = sqlx::query_as!(
        BillRow,
        r#"SELECT id, household_id, name, amount_min_minor, amount_max_minor, currency_code, first_due_date,
                  end_date, repeat_freq, skip, remind_days_before, remind_every_days,
                  active AS "active: bool", sort_order, notes, created_at, updated_at
           FROM bills WHERE household_id = ? ORDER BY sort_order, name"#,
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// A bill's periods (optionally of one status), oldest first.
/// Every period of the household's bills, for the export.
pub(crate) async fn all_periods_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<BillPeriodView>, AppError> {
    let household_id = ctx.household_id.to_string();
    let rows = sqlx::query_as!(
        PeriodRow,
        "SELECT id, bill_id, due_date, status, paid_split_id, paid_at, last_reminded_on, reminder_count, created_at, updated_at
         FROM bill_periods WHERE household_id = ? ORDER BY bill_id, due_date",
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

async fn period_rows(
    conn: &mut SqliteConnection,
    bill_id: &str,
    status: Option<&str>,
) -> Result<Vec<PeriodRow>, AppError> {
    let rows = sqlx::query_as!(
        PeriodRow,
        "SELECT id, bill_id, due_date, status, paid_split_id, paid_at, last_reminded_on, reminder_count, created_at, updated_at
         FROM bill_periods WHERE bill_id = ?1 AND (?2 IS NULL OR status = ?2) ORDER BY due_date",
        bill_id,
        status,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

async fn period_row(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: BillPeriodId,
) -> Result<PeriodRow, AppError> {
    let (household_id, id) = (ctx.household_id.to_string(), id.to_string());
    sqlx::query_as!(
        PeriodRow,
        "SELECT id, bill_id, due_date, status, paid_split_id, paid_at, last_reminded_on, reminder_count, created_at, updated_at
         FROM bill_periods WHERE household_id = ? AND id = ?",
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}
