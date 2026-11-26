//! Credit card plans (Phase 12, ARCHITECTURE.md 5.4f): a monthly budget per
//! card, what is still available, moving a transaction to the next month,
//! and the settlement transfer the server keeps in sync.

mod cycle_repo;
mod cycles;
mod moves;
mod repo;
mod sync;
mod view;

use saldo_core::card_cycle::Cycle;
use saldo_core::{AccountId, Currency, Money};
use sqlx::{SqliteConnection, SqlitePool};

use crate::clock::{SharedClock, format_timestamp};
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::identity::today_in;
use crate::transactions::TransactionService;
use repo::{AccountFacts, PlanRow};
pub use sync::{SyncFailure, SyncReport};
pub use view::{
    CardCycleDetailView, CardCycleView, CardLimitView, CardMoveRecordView, CardMoveView,
    CardSplitView, CardSummaryView, CounterpartyView, CreditCardPlanView, SettlementView,
};

const PLAN_ENTITY: &str = "credit_card_plan";
const CREDIT_CARD_ROLE: &str = "credit_card";

/// A plan as the client sends it.
#[derive(Debug, Clone)]
pub struct PlanInput {
    /// A decimal string in the card's currency. Required for a new plan; on
    /// an existing one it sets the budget from the current month on.
    pub monthly_limit: Option<String>,
    pub settlement_account_id: AccountId,
    pub settlement_day: i64,
}

#[derive(Clone)]
pub struct CreditCardService {
    pool: SqlitePool,
    clock: SharedClock,
    transactions: TransactionService,
}

impl CreditCardService {
    pub fn new(pool: SqlitePool, clock: SharedClock, transactions: TransactionService) -> Self {
        Self {
            pool,
            clock,
            transactions,
        }
    }

    pub async fn get_plan(
        &self,
        ctx: &HouseholdCtx,
        account_id: AccountId,
    ) -> Result<CreditCardPlanView, AppError> {
        let mut conn = self.pool.acquire().await?;
        let household_id = ctx.household_id.to_string();
        let row = repo::plan(&mut conn, &household_id, &account_id.to_string())
            .await?
            .ok_or(AppError::NotFound)?;
        plan_view(&mut conn, &household_id, row).await
    }

    /// Creates or replaces a card's plan. Returns the plan and whether it is
    /// new.
    pub async fn put_plan(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        account_id: AccountId,
        input: PlanInput,
    ) -> Result<(CreditCardPlanView, bool), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let card = card_account(&mut tx, &household_id, &account_id.to_string()).await?;
        let currency = currencies::find(&mut tx, &card.currency_code, "currency").await?;
        let paying = input.settlement_account_id.to_string();
        check_paying_account(&mut tx, &household_id, &card, &paying).await?;
        let day = check_day(input.settlement_day)?;
        let limit = input
            .monthly_limit
            .as_deref()
            .map(|text| parse_limit(text, &currency))
            .transpose()?;
        let now = format_timestamp(self.clock.now());
        let current = Cycle::of(today_in(&mut tx, ctx, &*self.clock).await?).to_string();
        let existing = repo::plan(&mut tx, &household_id, &card.id).await?;
        let created = existing.is_none();
        let row = match existing {
            Some(old) => PlanRow {
                settlement_account_id: paying,
                settlement_day: i64::from(day),
                updated_at: now.clone(),
                ..old
            },
            None => PlanRow {
                account_id: card.id.clone(),
                settlement_account_id: paying,
                settlement_day: i64::from(day),
                starts_cycle: current.clone(),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        };
        if created {
            let limit = limit
                .ok_or_else(|| AppError::invalid("monthly_limit", "is required for a new plan"))?;
            repo::insert_plan(&mut tx, &household_id, &row).await?;
            repo::upsert_limit(&mut tx, &household_id, &card.id, &current, limit, &now).await?;
        } else {
            repo::update_plan(&mut tx, &household_id, &row).await?;
            if let Some(limit) = limit {
                repo::upsert_limit(&mut tx, &household_id, &card.id, &current, limit, &now).await?;
            }
        }
        let view = plan_view(&mut tx, &household_id, row).await?;
        let verb = if created { "created" } else { "updated" };
        self.emit(&mut tx, ctx, origin, &view, verb).await?;
        tx.commit().await?;
        Ok((view, created))
    }

    /// Sets the budget from `from_cycle` on (decision 13): never for a month
    /// before the current one, so earlier months keep their budget.
    pub async fn set_limit(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        account_id: AccountId,
        from_cycle: Cycle,
        amount: &str,
    ) -> Result<CreditCardPlanView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let card_id = account_id.to_string();
        let row = repo::plan(&mut tx, &household_id, &card_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let card = card_account(&mut tx, &household_id, &card_id).await?;
        let currency = currencies::find(&mut tx, &card.currency_code, "currency").await?;
        let limit = parse_limit(amount, &currency)?;
        let current = Cycle::of(today_in(&mut tx, ctx, &*self.clock).await?);
        if from_cycle < current {
            return Err(AppError::invalid(
                "cycle",
                format!("must not be before the current month ({current})"),
            ));
        }
        let now = format_timestamp(self.clock.now());
        let from = from_cycle.to_string();
        repo::upsert_limit(&mut tx, &household_id, &card_id, &from, limit, &now).await?;
        let view = plan_view(&mut tx, &household_id, row).await?;
        self.emit(&mut tx, ctx, origin, &view, "updated").await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Deletes the plan (decision 14): the transfers the server still keeps
    /// in sync and that are not due yet are deleted; booked and hand-managed
    /// ones stay. Moves, budgets and month rows go with the plan.
    pub async fn delete_plan(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        account_id: AccountId,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let card_id = account_id.to_string();
        let row = repo::plan(&mut tx, &household_id, &card_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let today = today_in(&mut tx, ctx, &*self.clock).await?;
        let day = check_day(row.settlement_day)?;
        for cycle_row in cycle_repo::cycles(&mut tx, &household_id, &card_id).await? {
            let cycle: Cycle = parse_stored(&cycle_row.cycle)?;
            let open = cycle_row.state == "synced" && today < cycle.settlement_date(day);
            let Some(group_id) = cycle_row.group_id.filter(|_| open) else {
                continue;
            };
            if sync::group_exists(&mut tx, &household_id, &group_id).await? {
                let group_id = parse_stored(&group_id)?;
                self.transactions
                    .delete_in(&mut tx, ctx, Origin::Scheduler, group_id)
                    .await?;
            }
        }
        let view = plan_view(&mut tx, &household_id, row).await?;
        repo::delete_plan(&mut tx, &household_id, &card_id).await?;
        self.emit(&mut tx, ctx, origin, &view, "deleted").await?;
        tx.commit().await?;
        Ok(())
    }

    async fn emit(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        view: &CreditCardPlanView,
        verb: &str,
    ) -> Result<(), AppError> {
        let id = view.account_id.clone();
        let event = NewEvent::new(&*self.clock, PLAN_ENTITY, verb, id, view)?;
        append_event(conn, ctx, origin, event).await?;
        Ok(())
    }
}

/// The account if it is the household's credit card (404 when not the
/// household's, 422 for another kind of account).
async fn card_account(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
) -> Result<AccountFacts, AppError> {
    let card = repo::account(conn, household_id, account_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if card.account_type != "asset" || card.asset_role.as_deref() != Some(CREDIT_CARD_ROLE) {
        return Err(AppError::invalid(
            "account_id",
            "only asset accounts with the credit card role can have a plan",
        ));
    }
    Ok(card)
}

/// Decision 5: an active asset account of the household, not the card, in
/// the card's currency.
async fn check_paying_account(
    conn: &mut SqliteConnection,
    household_id: &str,
    card: &AccountFacts,
    paying_id: &str,
) -> Result<(), AppError> {
    let field = "settlement_account_id";
    let paying = repo::account(conn, household_id, paying_id)
        .await?
        .ok_or_else(|| AppError::invalid(field, "is not an account of this household"))?;
    if paying.id == card.id {
        return Err(AppError::invalid(field, "must not be the card itself"));
    }
    if paying.account_type != "asset" || !paying.active {
        return Err(AppError::invalid(field, "must be an active asset account"));
    }
    if paying.currency_code != card.currency_code {
        return Err(AppError::invalid(
            field,
            format!("must be in the card's currency ({})", card.currency_code),
        ));
    }
    Ok(())
}

fn check_day(day: i64) -> Result<u8, AppError> {
    u8::try_from(day)
        .ok()
        .filter(|d| (1..=31).contains(d))
        .ok_or_else(|| AppError::invalid("settlement_day", "must be between 1 and 31"))
}

fn parse_limit(text: &str, currency: &Currency) -> Result<i64, AppError> {
    let field = "monthly_limit";
    let amount = Money::parse_decimal(text, currency)
        .map_err(|e| AppError::invalid(field, e.to_string()))?;
    if !amount.is_positive() {
        return Err(AppError::invalid(field, "must be more than zero"));
    }
    Ok(amount.minor())
}

async fn plan_view(
    conn: &mut SqliteConnection,
    household_id: &str,
    row: PlanRow,
) -> Result<CreditCardPlanView, AppError> {
    let card = repo::account(conn, household_id, &row.account_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let currency = currencies::find(conn, &card.currency_code, "currency").await?;
    let limits = repo::limits(conn, household_id, &row.account_id)
        .await?
        .into_iter()
        .map(|(from_cycle, minor)| CardLimitView {
            from_cycle,
            amount: Money::new(minor, currency.code()).to_decimal_string(&currency),
        })
        .collect();
    Ok(CreditCardPlanView {
        account_id: row.account_id,
        currency: card.currency_code,
        settlement_account_id: row.settlement_account_id,
        settlement_day: check_day(row.settlement_day)?,
        starts_cycle: row.starts_cycle,
        limits,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Why an account change would break a plan, if it would: used by the
/// account service before an update (`can_be_card`, `can_pay`: whether the
/// changed account still could be a card or pay one) or a delete (both
/// false).
pub(crate) async fn account_change_conflict(
    conn: &mut SqliteConnection,
    household_id: &str,
    account_id: &str,
    can_be_card: bool,
    can_pay: bool,
) -> Result<Option<&'static str>, AppError> {
    let (is_card, pays) = repo::plan_roles(conn, household_id, account_id).await?;
    if is_card && !can_be_card {
        return Ok(Some(
            "the account has a credit card plan; remove the plan first",
        ));
    }
    if pays && !can_pay {
        return Ok(Some(
            "the account pays a credit card; choose another paying account first",
        ));
    }
    Ok(None)
}

/// Every plan of the household with its moves, for the export.
pub(crate) async fn all_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<(Vec<CreditCardPlanView>, Vec<CardMoveRecordView>), AppError> {
    let household_id = ctx.household_id.to_string();
    let mut plans = Vec::new();
    for row in repo::plans(conn, &household_id).await? {
        plans.push(plan_view(conn, &household_id, row).await?);
    }
    let moves = repo::all_moves(conn, &household_id)
        .await?
        .into_iter()
        .map(|(split_id, account_id, created_at)| CardMoveRecordView {
            split_id,
            account_id,
            created_at,
        })
        .collect();
    Ok((plans, moves))
}
