//! Keeping each month's settlement transfer in sync (decisions 9 and 10),
//! run by the `card-settlements` background task.

use saldo_core::card_cycle::{
    self, CardEntry, Change, Cycle, SettlementInput, SettlementState, SettlementStep, Stored,
    Transfer,
};
use saldo_core::{AccountId, GroupId};
use sqlx::SqliteConnection;

use super::CreditCardService;
use super::cycle_repo::{self, CycleRow};
use super::cycles::{self, Card};
use super::repo;
use super::view::CardMoveView;
use crate::ctx::{HouseholdCtx, Origin};
use crate::error::{AppError, parse_stored};
use crate::identity::today_in;
use crate::transactions::{SplitInput, TransactionInput, input_from_view};

/// What one run of [`CreditCardService::sync_settlements`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SyncReport {
    /// Transfers created, updated or deleted, and stale moves removed.
    pub changes: usize,
    /// Cards that could not be synced (for example the paying account was
    /// archived); the next run tries again.
    pub failures: Vec<SyncFailure>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncFailure {
    pub account_id: String,
    pub message: String,
}

impl CreditCardService {
    /// Syncs every card of the household, one database transaction per
    /// card, so one failing card does not stop the others.
    pub async fn sync_settlements(&self, ctx: &HouseholdCtx) -> Result<SyncReport, AppError> {
        let plans = {
            let mut conn = self.pool.acquire().await?;
            repo::plans(&mut conn, &ctx.household_id.to_string()).await?
        };
        let mut report = SyncReport::default();
        for plan in plans {
            match self.sync_card(ctx, &plan.account_id).await {
                Ok(changes) => report.changes += changes,
                Err(error) => report.failures.push(SyncFailure {
                    account_id: plan.account_id,
                    message: error.to_string(),
                }),
            }
        }
        Ok(report)
    }

    async fn sync_card(&self, ctx: &HouseholdCtx, card_id: &str) -> Result<usize, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let today = today_in(&mut tx, ctx, &*self.clock).await?;
        let mut changes = self.drop_stale_moves(&mut tx, ctx, card_id).await?;
        let card = Card::load(&mut tx, &household_id, card_id).await?;
        let months = synced_months(&card, Cycle::of(today))?;
        if let (Some(first), Some(last)) = (months.first(), months.last()) {
            let rows = cycles::splits_for(&mut tx, &household_id, card_id, *first, *last).await?;
            let entries = cycles::entries(&rows)?;
            for cycle in months {
                changes += self
                    .sync_month(&mut tx, ctx, &card, cycle, &entries, today)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(changes)
    }

    /// Removes the moves whose split no longer counts toward the card.
    async fn drop_stale_moves(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        card_id: &str,
    ) -> Result<usize, AppError> {
        let household_id = ctx.household_id.to_string();
        let stale = repo::stale_moves(conn, &household_id, card_id).await?;
        for (split_id, date) in &stale {
            repo::delete_move(conn, &household_id, split_id).await?;
            let own = Cycle::of(parse_stored(date)?);
            let view = CardMoveView {
                account_id: card_id.to_string(),
                split_id: split_id.clone(),
                from_cycle: own.next().to_string(),
                to_cycle: own.to_string(),
            };
            self.emit_move(conn, ctx, Origin::Scheduler, &view, "deleted")
                .await?;
        }
        Ok(stale.len())
    }

    /// Applies the decision table to one month; returns 1 when the transfer
    /// changed.
    async fn sync_month(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        card: &Card,
        cycle: Cycle,
        entries: &[CardEntry],
        today: chrono::NaiveDate,
    ) -> Result<usize, AppError> {
        let household_id = ctx.household_id.to_string();
        let row = card.rows.get(&cycle);
        let group_id = row.and_then(|r| r.group_id.clone());
        let (stored, reconciled) = match &group_id {
            Some(id) => stored_transfer(conn, &household_id, id, card).await?,
            None => (Stored::Missing, false),
        };
        let written = row.map(written_of).transpose()?.flatten();
        let spent = card_cycle::cycle_spending(entries, cycle)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let step = card_cycle::settlement_step(SettlementInput {
            state: card.state(cycle)?,
            written,
            stored,
            reconciled,
            spent,
            paying_account: parse_stored(&card.plan.settlement_account_id)?,
            settlement_date: cycle.settlement_date(card.day),
            today,
        });
        let (group_id, written) = self
            .apply(conn, ctx, card, cycle, group_id, written, step)
            .await?;
        let new_row = cycle_row(cycle, step.next_state, group_id, written);
        let unchanged = match row {
            Some(old) => *old == new_row,
            None => new_row.state == "synced" && new_row.group_id.is_none(),
        };
        if !unchanged {
            cycle_repo::upsert_cycle(conn, &household_id, &card.facts.id, &new_row).await?;
        }
        Ok(usize::from(step.change != Change::None))
    }

    /// Carries out the step's change; returns the month's group and what
    /// the server wrote afterwards.
    #[allow(clippy::too_many_arguments)] // all needed; a struct would only rename them
    async fn apply(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        card: &Card,
        cycle: Cycle,
        group_id: Option<String>,
        written: Option<Transfer>,
        step: SettlementStep,
    ) -> Result<(Option<String>, Option<Transfer>), AppError> {
        let origin = Origin::Scheduler;
        match (step.change, group_id) {
            (Change::None, group_id) => Ok((group_id, written)),
            (Change::Create(t), _) => {
                let input = TransactionInput {
                    title: None,
                    splits: vec![settlement_split(card, cycle, t)?],
                };
                let view = self
                    .transactions
                    .write_create_in(conn, ctx, origin, input)
                    .await?;
                Ok((Some(view.id), Some(t)))
            }
            (Change::Update(t), Some(id)) => {
                let group: GroupId = parse_stored(&id)?;
                let view = self.transactions.get_in(conn, ctx, group).await?;
                // Keep what the owner added (category, tags, notes): only
                // the parts the server owns change.
                let mut input = input_from_view(&view)?;
                if let Some(split) = input.splits.first_mut() {
                    let wanted = settlement_split(card, cycle, t)?;
                    split.date = wanted.date;
                    split.source_id = wanted.source_id;
                    split.amount = wanted.amount;
                    split.description = wanted.description;
                }
                self.transactions
                    .write_replace_in(conn, ctx, origin, group, input)
                    .await?;
                Ok((Some(id), Some(t)))
            }
            (Change::Delete, Some(id)) => {
                let group: GroupId = parse_stored(&id)?;
                self.transactions
                    .delete_in(conn, ctx, origin, group)
                    .await?;
                Ok((None, None))
            }
            (_, None) => Err(AppError::Internal(format!(
                "settlement of {cycle} has no group to change"
            ))),
        }
    }
}

/// The months the server still manages, oldest first: from the plan's start
/// to the current month, skipping frozen and detached ones.
fn synced_months(card: &Card, current: Cycle) -> Result<Vec<Cycle>, AppError> {
    let mut months = Vec::new();
    let mut cycle = card.starts;
    while cycle <= current {
        if card.state(cycle)? == SettlementState::Synced {
            months.push(cycle);
        }
        cycle = cycle.next();
    }
    Ok(months)
}

/// What the server last wrote for a month, from its row.
fn written_of(row: &CycleRow) -> Result<Option<Transfer>, AppError> {
    match (
        &row.written_amount_minor,
        &row.written_date,
        &row.written_source_id,
    ) {
        (Some(amount), Some(date), Some(source)) => Ok(Some(Transfer {
            amount: *amount,
            date: parse_stored(date)?,
            source: parse_stored(source)?,
        })),
        _ => Ok(None),
    }
}

fn cycle_row(
    cycle: Cycle,
    state: SettlementState,
    group_id: Option<String>,
    written: Option<Transfer>,
) -> CycleRow {
    // Keep the four columns together: a row without a group writes none.
    let written = group_id.as_ref().and(written);
    CycleRow {
        cycle: cycle.to_string(),
        state: state.as_str().to_string(),
        group_id: written.and(group_id),
        written_amount_minor: written.map(|t| t.amount),
        written_date: written.map(|t| t.date.to_string()),
        written_source_id: written.map(|t| t.source.to_string()),
    }
}

/// What the database holds for the month's group, and whether it is
/// reconciled.
async fn stored_transfer(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_id: &str,
    card: &Card,
) -> Result<(Stored, bool), AppError> {
    let splits = cycle_repo::settlement_splits(conn, household_id, group_id).await?;
    let reconciled = splits.iter().any(|s| s.reconciled);
    let [split] = splits.as_slice() else {
        let stored = if splits.is_empty() {
            Stored::Missing
        } else {
            Stored::Other
        };
        return Ok((stored, reconciled));
    };
    let shaped = split.kind == "transfer"
        && split.destination_account_id == card.facts.id
        && split.currency_code == card.facts.currency_code
        && split.foreign_amount_minor.is_none();
    if !shaped {
        return Ok((Stored::Other, reconciled));
    }
    let transfer = Transfer {
        amount: split.amount_minor,
        date: parse_stored(&split.date)?,
        source: parse_stored(&split.source_account_id)?,
    };
    Ok((Stored::Transfer(transfer), reconciled))
}

/// The settlement split for a month (decision 11).
fn settlement_split(card: &Card, cycle: Cycle, t: Transfer) -> Result<SplitInput, AppError> {
    let destination: AccountId = parse_stored(&card.facts.id)?;
    Ok(SplitInput {
        id: None,
        date: t.date,
        description: format!(
            "{} — {}",
            card.facts.name,
            cycle.first_day().format("%B %Y")
        ),
        source_id: t.source,
        destination_id: destination,
        amount: card.money(t.amount),
        currency: None,
        foreign_amount: None,
        foreign_currency: None,
        category_id: None,
        budget_id: None,
        bill_id: None,
        tag_ids: Vec::new(),
        reconciled: false,
        notes: None,
        meta: serde_json::Map::new(),
    })
}

/// Whether the group still exists in the household.
pub(super) async fn group_exists(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_id: &str,
) -> Result<bool, AppError> {
    Ok(!cycle_repo::settlement_splits(conn, household_id, group_id)
        .await?
        .is_empty())
}
