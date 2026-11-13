//! Rules: groups of rules that change transactions when they are saved
//! (ARCHITECTURE.md 4.3 and 5.4).

mod check;
pub(crate) mod engine;
mod names;
mod store;

use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::{SharedClock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::identity::required;
use crate::transactions::{self, TransactionService};

pub use check::TRIGGER_ON;
pub use names::{RuleNames, names_in};
pub use saldo_core::rules::{ACTION_KINDS, TRIGGER_KINDS};
pub use store::{ActionView, RuleGroupView, RuleView, TriggerView};

/// Matches returned by a dry run, at most this many.
pub const TEST_LIMIT: usize = 100;

#[derive(Debug, Clone, Default)]
pub struct NewRuleGroup {
    pub title: String,
    pub description: Option<String>,
    pub sort_order: i64,
    pub active: bool,
}

/// `None` keeps a value; `description: Some(None)` clears it.
#[derive(Debug, Clone, Default)]
pub struct RuleGroupUpdate {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub sort_order: Option<i64>,
    pub active: Option<bool>,
}

/// A whole rule: created with POST, replaced with PUT.
#[derive(Debug, Clone)]
pub struct RuleInput {
    pub rule_group_id: String,
    pub title: String,
    pub description: Option<String>,
    pub trigger_on: String,
    pub strict: bool,
    pub stop_processing: bool,
    pub active: bool,
    pub sort_order: i64,
    pub triggers: Vec<TriggerView>,
    pub actions: Vec<ActionView>,
}

/// One split a rule would change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct RuleMatch {
    pub group_id: String,
    pub split_id: String,
    pub date: String,
    pub description: String,
    pub amount: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct RuleTestResult {
    /// Every matching split.
    pub total: usize,
    /// The first matches (at most 100), newest first.
    pub matches: Vec<RuleMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct RuleApplyResult {
    /// Transaction groups with a matching split.
    pub matched: usize,
    /// Groups that the rule actually changed.
    pub changed: usize,
}

#[derive(Clone)]
pub struct RuleService {
    pool: SqlitePool,
    clock: SharedClock,
    transactions: TransactionService,
}

impl RuleService {
    pub fn new(pool: SqlitePool, clock: SharedClock, transactions: TransactionService) -> Self {
        Self {
            pool,
            clock,
            transactions,
        }
    }

    pub async fn create_group(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewRuleGroup,
    ) -> Result<RuleGroupView, AppError> {
        let mut tx = self.pool.begin().await?;
        let view = self.create_group_in(&mut tx, ctx, origin, input).await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn create_group_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: NewRuleGroup,
    ) -> Result<RuleGroupView, AppError> {
        let title = required("title", &input.title)?;
        let (id, household_id, now) = (
            new_id(&*self.clock).to_string(),
            ctx.household_id.to_string(),
            format_timestamp(self.clock.now()),
        );
        sqlx::query!(
            "INSERT INTO rule_groups (id, household_id, title, description, sort_order, active, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            id, household_id, title, input.description, input.sort_order, input.active, now, now,
        )
        .execute(&mut *conn)
        .await?;
        let view = store::group(&mut *conn, &household_id, &id).await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "rule_group", "created", id, &view)?,
        )
        .await?;
        Ok(view)
    }

    pub async fn update_group(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
        update: RuleGroupUpdate,
    ) -> Result<RuleGroupView, AppError> {
        let mut tx = self.pool.begin().await?;
        let view = self
            .update_group_in(&mut tx, ctx, origin, id, update)
            .await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn update_group_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
        update: RuleGroupUpdate,
    ) -> Result<RuleGroupView, AppError> {
        let household_id = ctx.household_id.to_string();
        let current = store::group(&mut *conn, &household_id, id).await?;
        let title = match update.title {
            Some(title) => required("title", &title)?,
            None => current.title,
        };
        let description = update.description.unwrap_or(current.description);
        let sort_order = update.sort_order.unwrap_or(current.sort_order);
        let active = update.active.unwrap_or(current.active);
        let now = format_timestamp(self.clock.now());
        sqlx::query!(
            "UPDATE rule_groups SET title = ?, description = ?, sort_order = ?, active = ?, updated_at = ?
             WHERE household_id = ? AND id = ?",
            title, description, sort_order, active, now, household_id, id,
        )
        .execute(&mut *conn)
        .await?;
        let view = store::group(&mut *conn, &household_id, id).await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "rule_group", "updated", id.to_string(), &view)?,
        )
        .await?;
        Ok(view)
    }

    /// Deletes a group and its rules.
    pub async fn delete_group(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
        let household_id = ctx.household_id.to_string();
        let view = store::group(&mut tx, &household_id, id).await?;
        sqlx::query!(
            "DELETE FROM rule_groups WHERE household_id = ? AND id = ?",
            household_id,
            id
        )
        .execute(&mut *tx)
        .await?;
        append_event(
            &mut tx,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "rule_group", "deleted", id.to_string(), &view)?,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_group(&self, ctx: &HouseholdCtx, id: &str) -> Result<RuleGroupView, AppError> {
        let mut conn = self.pool.acquire().await?;
        store::group(&mut conn, &ctx.household_id.to_string(), id).await
    }

    /// Rule groups in evaluation order.
    pub async fn list_groups(&self, ctx: &HouseholdCtx) -> Result<Vec<RuleGroupView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        store::groups(&mut conn, &ctx.household_id.to_string()).await
    }

    pub async fn create_rule(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: RuleInput,
    ) -> Result<RuleView, AppError> {
        let mut tx = self.pool.begin().await?;
        let view = self.create_rule_in(&mut tx, ctx, origin, input).await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn create_rule_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: RuleInput,
    ) -> Result<RuleView, AppError> {
        let id = new_id(&*self.clock).to_string();
        self.write_rule(conn, ctx, &id, &input, true).await?;
        let view = store::rule(&mut *conn, &ctx.household_id.to_string(), &id).await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "rule", "created", id, &view)?,
        )
        .await?;
        Ok(view)
    }

    pub async fn replace_rule(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
        input: RuleInput,
    ) -> Result<RuleView, AppError> {
        let mut tx = self.pool.begin().await?;
        let view = self
            .replace_rule_in(&mut tx, ctx, origin, id, input)
            .await?;
        tx.commit().await?;
        Ok(view)
    }

    pub async fn replace_rule_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
        input: RuleInput,
    ) -> Result<RuleView, AppError> {
        store::rule(&mut *conn, &ctx.household_id.to_string(), id).await?;
        self.write_rule(conn, ctx, id, &input, false).await?;
        let view = store::rule(&mut *conn, &ctx.household_id.to_string(), id).await?;
        append_event(
            &mut *conn,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "rule", "updated", id.to_string(), &view)?,
        )
        .await?;
        Ok(view)
    }

    pub async fn delete_rule(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: &str,
    ) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await?;
        let household_id = ctx.household_id.to_string();
        let view = store::rule(&mut tx, &household_id, id).await?;
        sqlx::query!(
            "DELETE FROM rules WHERE household_id = ? AND id = ?",
            household_id,
            id
        )
        .execute(&mut *tx)
        .await?;
        append_event(
            &mut tx,
            ctx,
            origin,
            NewEvent::new(&*self.clock, "rule", "deleted", id.to_string(), &view)?,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_rule(&self, ctx: &HouseholdCtx, id: &str) -> Result<RuleView, AppError> {
        let mut conn = self.pool.acquire().await?;
        store::rule(&mut conn, &ctx.household_id.to_string(), id).await
    }

    pub async fn get_rule_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: &str,
    ) -> Result<RuleView, AppError> {
        store::rule(conn, &ctx.household_id.to_string(), id).await
    }

    pub async fn get_group_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: &str,
    ) -> Result<RuleGroupView, AppError> {
        store::group(conn, &ctx.household_id.to_string(), id).await
    }

    /// Rules in evaluation order, optionally of one group.
    pub async fn list_rules(
        &self,
        ctx: &HouseholdCtx,
        group_id: Option<&str>,
    ) -> Result<Vec<RuleView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        store::rules(&mut conn, &ctx.household_id.to_string(), group_id).await
    }

    /// Dry run: the splits the rule would change, without changing anything.
    pub async fn test_rule(
        &self,
        ctx: &HouseholdCtx,
        id: &str,
    ) -> Result<RuleTestResult, AppError> {
        let mut conn = self.pool.acquire().await?;
        let rule = self.runnable(&mut conn, ctx, id).await?;
        let places = engine::decimal_places(&mut conn).await?;
        let mut matches = Vec::new();
        for group in transactions::all_in(&mut conn, ctx).await?.iter().rev() {
            for split in &group.splits {
                let before = engine::rule_split(split, &places)?;
                if saldo_core::rules::matches(&rule.rule, &before) {
                    matches.push(RuleMatch {
                        group_id: group.id.clone(),
                        split_id: split.id.clone(),
                        date: split.date.clone(),
                        description: split.description.clone(),
                        amount: split.amount.clone(),
                        currency: split.currency.clone(),
                    });
                }
            }
        }
        let total = matches.len();
        matches.truncate(TEST_LIMIT);
        Ok(RuleTestResult { total, matches })
    }

    /// Runs the rule over every transaction now and saves the changes with
    /// `Origin::Rule`, in one database transaction.
    pub async fn apply_rule(
        &self,
        ctx: &HouseholdCtx,
        id: &str,
    ) -> Result<RuleApplyResult, AppError> {
        let mut tx = self.pool.begin().await?;
        let rule = self.runnable(&mut tx, ctx, id).await?;
        let rules = [rule];
        let places = engine::decimal_places(&mut tx).await?;
        let (mut matched, mut changed) = (0, 0);
        for group in transactions::all_in(&mut tx, ctx).await? {
            let any = group
                .splits
                .iter()
                .map(|s| engine::rule_split(s, &places))
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .any(|s| saldo_core::rules::matches(&rules[0].rule, s));
            if !any {
                continue;
            }
            matched += 1;
            if let Some(input) = engine::changes(&mut tx, &rules, &group).await? {
                let group_id = group
                    .id
                    .parse()
                    .map_err(|_| AppError::Internal(format!("bad group id {}", group.id)))?;
                self.transactions
                    .write_replace_in(&mut tx, ctx, Origin::Rule, group_id, input)
                    .await?;
                changed += 1;
            }
        }
        tx.commit().await?;
        Ok(RuleApplyResult { matched, changed })
    }

    async fn runnable(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: &str,
    ) -> Result<engine::LoadedRule, AppError> {
        let view = store::rule(conn, &ctx.household_id.to_string(), id).await?;
        engine::to_rule(&view)
            .map_err(|why| AppError::invalid("rule", format!("cannot run: {why}")))
    }

    async fn write_rule(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: &str,
        input: &RuleInput,
        new: bool,
    ) -> Result<(), AppError> {
        let household_id = ctx.household_id.to_string();
        let title = required("title", &input.title)?;
        if !TRIGGER_ON.contains(&input.trigger_on.as_str()) {
            return Err(AppError::invalid(
                "trigger_on",
                "must be create, update or manual",
            ));
        }
        store::group(&mut *conn, &household_id, &input.rule_group_id)
            .await
            .map_err(|_| AppError::invalid("rule_group_id", "no rule group with this ID"))?;
        check::check(
            &mut *conn,
            &household_id,
            input.active,
            &input.triggers,
            &input.actions,
        )
        .await?;
        let now = format_timestamp(self.clock.now());
        if new {
            sqlx::query!(
                "INSERT INTO rules (id, household_id, rule_group_id, title, description, trigger_on, strict,
                    stop_processing, active, sort_order, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                id, household_id, input.rule_group_id, title, input.description, input.trigger_on, input.strict,
                input.stop_processing, input.active, input.sort_order, now, now,
            )
            .execute(&mut *conn)
            .await?;
        } else {
            sqlx::query!(
                "UPDATE rules SET rule_group_id = ?, title = ?, description = ?, trigger_on = ?, strict = ?,
                    stop_processing = ?, active = ?, sort_order = ?, updated_at = ?
                 WHERE household_id = ? AND id = ?",
                input.rule_group_id, title, input.description, input.trigger_on, input.strict,
                input.stop_processing, input.active, input.sort_order, now, household_id, id,
            )
            .execute(&mut *conn)
            .await?;
        }
        let clock = self.clock.clone();
        let mut ids = move || new_id(&*clock).to_string();
        store::write_parts(conn, &mut ids, id, &input.triggers, &input.actions).await
    }
}
