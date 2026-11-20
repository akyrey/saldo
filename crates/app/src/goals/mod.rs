//! Net worth goals (Phase 10): a target net worth, for the household or one
//! member, optionally by a date. Progress is computed when read from the net
//! worth report; the alert scheduler records when a goal is reached.

mod view;

use chrono::NaiveDate;
use saldo_core::{Money, UserId};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity::{self, required, today_in};

pub use view::{GoalProgressView, NetWorthGoalView, TREND_MONTHS};
use view::{Row, scope_of, trend};

const ENTITY: &str = "net_worth_goal";
const DUPLICATE: &str = "a goal with this name exists";

#[derive(Debug, Clone, Default)]
pub struct NewGoal {
    pub name: String,
    /// `None`: the whole household.
    pub owner_user_id: Option<UserId>,
    /// A decimal amount; may be below today's net worth (paying debt off).
    pub target_amount: String,
    /// Default: the household's primary currency.
    pub currency: Option<String>,
    pub target_date: Option<NaiveDate>,
    pub notes: Option<String>,
}

/// `None` keeps a value; `Some(None)` clears an optional one.
#[derive(Debug, Clone, Default)]
pub struct GoalUpdate {
    pub name: Option<String>,
    pub target_amount: Option<String>,
    pub target_date: Option<Option<NaiveDate>>,
    pub active: Option<bool>,
    pub notes: Option<Option<String>>,
}

#[derive(Clone)]
pub struct GoalService {
    pool: SqlitePool,
    clock: SharedClock,
}

impl GoalService {
    pub fn new(pool: SqlitePool, clock: SharedClock) -> Self {
        Self { pool, clock }
    }

    /// Creates the goal, starting from today's net worth in its scope.
    pub async fn create(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewGoal,
    ) -> Result<NetWorthGoalView, AppError> {
        let name = required("name", &input.name)?;
        let mut conn = self.pool.acquire().await?;
        let code = match input.currency.as_deref().map(str::trim) {
            Some(code) if !code.is_empty() => code.to_string(),
            _ => identity::primary_currency(&mut conn, ctx).await?,
        };
        let currency = currencies::find(&mut conn, &code, "currency").await?;
        let target = amount("target_amount", &input.target_amount, &currency)?;
        let owner = input.owner_user_id.map(|id| id.to_string());
        check_member(&mut conn, ctx, owner.as_deref()).await?;
        let today = today_in(&mut conn, ctx, &*self.clock).await?;
        let scope = scope_of(owner.as_deref())?;
        let (start, _) = trend(&mut conn, ctx, scope, &currency, today).await?;
        drop(conn);

        let id = new_id(&*self.clock).to_string();
        let household_id = ctx.household_id.to_string();
        let now = format_timestamp(self.clock.now());
        let (code, due) = (
            currency.code().to_string(),
            input.target_date.map(|d| d.to_string()),
        );
        let mut tx = crate::db::begin(&self.pool).await?;
        sqlx::query!(
            "INSERT INTO net_worth_goals (id, household_id, name, owner_user_id, target_amount_minor,
                start_amount_minor, currency_code, target_date, notes, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            id, household_id, name, owner, target, start, code, due, input.notes, now, now,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let view = self
            .emit(&mut tx, ctx, origin, "created", &id, today)
            .await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn update(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: Uuid,
        update: GoalUpdate,
    ) -> Result<NetWorthGoalView, AppError> {
        let id = id.to_string();
        let mut tx = crate::db::begin(&self.pool).await?;
        let row = find(&mut tx, ctx, &id).await?;
        let name = match update.name {
            Some(name) => required("name", &name)?,
            None => row.name,
        };
        let target = match update.target_amount {
            Some(text) => {
                let currency = currencies::find(&mut tx, &row.currency_code, "currency").await?;
                amount("target_amount", &text, &currency)?
            }
            None => row.target_amount_minor,
        };
        // A new target may not be reached yet: let the scheduler decide again.
        let reached_on = if target == row.target_amount_minor {
            row.reached_on
        } else {
            None
        };
        let due = match update.target_date {
            Some(date) => date.map(|d| d.to_string()),
            None => row.target_date,
        };
        let active = update.active.unwrap_or(row.active);
        let notes = update.notes.unwrap_or(row.notes);
        let household_id = ctx.household_id.to_string();
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "UPDATE net_worth_goals SET name = ?, target_amount_minor = ?, target_date = ?,
                reached_on = ?, active = ?, notes = ?, updated_at = ?
             WHERE household_id = ? AND id = ?",
            name,
            target,
            due,
            reached_on,
            active,
            notes,
            now,
            household_id,
            id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::conflict_on_unique(e, DUPLICATE))?;
        let today = today_in(&mut tx, ctx, &*self.clock).await?;
        let view = self
            .emit(&mut tx, ctx, origin, "updated", &id, today)
            .await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn delete(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: Uuid,
    ) -> Result<(), AppError> {
        let id = id.to_string();
        let mut tx = crate::db::begin(&self.pool).await?;
        let today = today_in(&mut tx, ctx, &*self.clock).await?;
        let row = find(&mut tx, ctx, &id).await?;
        let view = view::view(&mut tx, ctx, row, today).await?;
        let household_id = ctx.household_id.to_string();
        sqlx::query!(
            "DELETE FROM net_worth_goals WHERE household_id = ? AND id = ?",
            household_id,
            id
        )
        .execute(&mut *tx)
        .await?;
        let event = NewEvent::new(&*self.clock, ENTITY, "deleted", id, &view)?;
        append_event(&mut tx, ctx, origin, event).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(&self, ctx: &HouseholdCtx, id: Uuid) -> Result<NetWorthGoalView, AppError> {
        let mut conn = self.pool.acquire().await?;
        let today = today_in(&mut conn, ctx, &*self.clock).await?;
        let row = find(&mut conn, ctx, &id.to_string()).await?;
        view::view(&mut conn, ctx, row, today).await
    }

    /// Goals by name; `active` filters when given. Unpaginated: a household
    /// keeps a handful.
    pub async fn list(
        &self,
        ctx: &HouseholdCtx,
        active: Option<bool>,
    ) -> Result<Vec<NetWorthGoalView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let today = today_in(&mut conn, ctx, &*self.clock).await?;
        let household_id = ctx.household_id.to_string();
        let rows = sqlx::query_as!(
            Row,
            r#"SELECT id, name, owner_user_id, target_amount_minor, start_amount_minor, currency_code,
                      target_date, reached_on, active AS "active: bool", notes, created_at, updated_at
               FROM net_worth_goals WHERE household_id = ? AND (? IS NULL OR active = ?)
               ORDER BY name"#,
            household_id,
            active,
            active,
        )
        .fetch_all(&mut *conn)
        .await?;
        let mut views = Vec::with_capacity(rows.len());
        for row in rows {
            views.push(view::view(&mut conn, ctx, row, today).await?);
        }
        Ok(views)
    }

    async fn emit(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        action: &str,
        id: &str,
        today: NaiveDate,
    ) -> Result<NetWorthGoalView, AppError> {
        let row = find(&mut *conn, ctx, id).await?;
        let view = view::view(&mut *conn, ctx, row, today).await?;
        let event = NewEvent::new(&*self.clock, ENTITY, action, id.to_string(), &view)?;
        append_event(&mut *conn, ctx, origin, event).await?;
        Ok(view)
    }
}

/// Records that the alert scheduler saw the goal reached (no event of its
/// own: the `net_worth_goal.reached` alert is it).
pub(crate) async fn mark_reached(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    id: &str,
    today: NaiveDate,
) -> Result<(), AppError> {
    let (household_id, day) = (ctx.household_id.to_string(), today.to_string());
    sqlx::query!(
        "UPDATE net_worth_goals SET reached_on = ? WHERE household_id = ? AND id = ? AND reached_on IS NULL",
        day,
        household_id,
        id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Every goal of the household (for export), by name.
pub(crate) async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    today: NaiveDate,
) -> Result<Vec<NetWorthGoalView>, AppError> {
    let household_id = ctx.household_id.to_string();
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT id, name, owner_user_id, target_amount_minor, start_amount_minor, currency_code,
                  target_date, reached_on, active AS "active: bool", notes, created_at, updated_at
           FROM net_worth_goals WHERE household_id = ? ORDER BY name"#,
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        views.push(view::view(&mut *conn, ctx, row, today).await?);
    }
    Ok(views)
}

async fn find(conn: &mut SqliteConnection, ctx: &HouseholdCtx, id: &str) -> Result<Row, AppError> {
    let household_id = ctx.household_id.to_string();
    sqlx::query_as!(
        Row,
        r#"SELECT id, name, owner_user_id, target_amount_minor, start_amount_minor, currency_code,
                  target_date, reached_on, active AS "active: bool", notes, created_at, updated_at
           FROM net_worth_goals WHERE household_id = ? AND id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

fn amount(field: &str, text: &str, currency: &saldo_core::Currency) -> Result<i64, AppError> {
    Money::parse_decimal(text.trim(), currency)
        .map(|m| m.minor())
        .map_err(|e| AppError::invalid(field, e.to_string()))
}

async fn check_member(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    owner: Option<&str>,
) -> Result<(), AppError> {
    let Some(owner) = owner else {
        return Ok(());
    };
    let household_id = ctx.household_id.to_string();
    let member = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM household_members WHERE household_id = ? AND user_id = ?) AS "member!: bool""#,
        household_id,
        owner
    )
    .fetch_one(&mut *conn)
    .await?;
    if member {
        Ok(())
    } else {
        Err(AppError::invalid(
            "owner_user_id",
            "is not a member of the household",
        ))
    }
}
