//! Reading a card's months: spending, budget, available and settlement.

use std::collections::HashMap;

use chrono::NaiveDate;
use saldo_core::card_cycle::{self, CardEntry, Cycle, SettlementState};
use saldo_core::{AccountId, Currency, Money};
use sqlx::SqliteConnection;

use super::cycle_repo::{self, CycleRow};
use super::repo::{self, AccountFacts, CardSplitRow, PlanRow};
use super::view::{
    CardCycleDetailView, CardCycleView, CardSplitView, CardSummaryView, CounterpartyView,
    SettlementView,
};
use super::{CreditCardService, check_day};
use crate::ctx::HouseholdCtx;
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::identity::today_in;

/// How many months one `cycles` call may return.
pub const MAX_CYCLES: u32 = 36;
/// The months `cycles` returns without a range: the current one and five
/// before it.
pub const DEFAULT_CYCLES: u32 = 6;

/// Everything about one card that the month maths needs, loaded once.
pub(super) struct Card {
    pub facts: AccountFacts,
    pub currency: Currency,
    pub plan: PlanRow,
    pub day: u8,
    pub starts: Cycle,
    pub limits: Vec<(Cycle, i64)>,
    pub rows: HashMap<Cycle, CycleRow>,
}

impl Card {
    pub(super) async fn load(
        conn: &mut SqliteConnection,
        household_id: &str,
        card_id: &str,
    ) -> Result<Self, AppError> {
        let plan = repo::plan(conn, household_id, card_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let facts = repo::account(conn, household_id, card_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let currency = currencies::find(conn, &facts.currency_code, "currency").await?;
        let limits = repo::limits(conn, household_id, card_id)
            .await?
            .into_iter()
            .map(|(from, minor)| Ok((parse_stored::<Cycle>(&from)?, minor)))
            .collect::<Result<Vec<_>, AppError>>()?;
        let mut rows = HashMap::new();
        for row in cycle_repo::cycles(conn, household_id, card_id).await? {
            rows.insert(parse_stored::<Cycle>(&row.cycle)?, row);
        }
        Ok(Self {
            day: check_day(plan.settlement_day)?,
            starts: parse_stored(&plan.starts_cycle)?,
            facts,
            currency,
            plan,
            limits,
            rows,
        })
    }

    pub(super) fn money(&self, minor: i64) -> String {
        Money::new(minor, self.currency.code()).to_decimal_string(&self.currency)
    }

    /// The stored state of a month; a month without a row is synced.
    pub(super) fn state(&self, cycle: Cycle) -> Result<SettlementState, AppError> {
        match self.rows.get(&cycle) {
            Some(row) => SettlementState::parse(&row.state)
                .ok_or_else(|| AppError::Internal(format!("bad cycle state {:?}", row.state))),
            None => Ok(SettlementState::Synced),
        }
    }

    /// Whether the month no longer accepts moves (before the plan, frozen,
    /// or its payment day has come).
    pub(super) fn is_closed(&self, cycle: Cycle, today: NaiveDate) -> Result<bool, AppError> {
        if cycle < self.starts {
            return Ok(true);
        }
        let date = cycle.settlement_date(self.day);
        Ok(card_cycle::is_closed(self.state(cycle)?, date, today))
    }
}

/// The splits as core entries.
pub(super) fn entries(rows: &[CardSplitRow]) -> Result<Vec<CardEntry>, AppError> {
    rows.iter()
        .map(|r| {
            Ok(CardEntry {
                date: parse_stored(&r.date)?,
                amount: r.amount_minor,
                moved: r.moved,
            })
        })
        .collect()
}

/// The card's counting splits that can touch months `from` to `to`: dated
/// from the month before `from` (moved in) to the end of `to`.
pub(super) async fn splits_for(
    conn: &mut SqliteConnection,
    household_id: &str,
    card_id: &str,
    from: Cycle,
    to: Cycle,
) -> Result<Vec<CardSplitRow>, AppError> {
    let start = from.previous().first_day().to_string();
    let end = to.last_day().to_string();
    repo::card_splits(conn, household_id, card_id, &start, &end).await
}

impl CreditCardService {
    /// The card's months from `from` to `to`, newest first. Without a range:
    /// the current month and the five before it. Months before the plan
    /// started are left out.
    pub async fn cycles(
        &self,
        ctx: &HouseholdCtx,
        account_id: AccountId,
        from: Option<Cycle>,
        to: Option<Cycle>,
    ) -> Result<Vec<CardCycleView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let household_id = ctx.household_id.to_string();
        let card = Card::load(&mut conn, &household_id, &account_id.to_string()).await?;
        let today = today_in(&mut conn, ctx, &*self.clock).await?;
        let (from, to) = range(from, to, Cycle::of(today))?;
        let from = from.max(card.starts);
        if to < from {
            return Ok(Vec::new());
        }
        let rows = splits_for(&mut conn, &household_id, &card.facts.id, from, to).await?;
        let entries = entries(&rows)?;
        let mut views = Vec::new();
        let mut cycle = to;
        while cycle >= from {
            views.push(cycle_view(&mut conn, &household_id, &card, cycle, &entries).await?);
            cycle = cycle.previous();
        }
        Ok(views)
    }

    /// One month with its transactions.
    pub async fn cycle(
        &self,
        ctx: &HouseholdCtx,
        account_id: AccountId,
        cycle: Cycle,
    ) -> Result<CardCycleDetailView, AppError> {
        let mut conn = self.pool.acquire().await?;
        let household_id = ctx.household_id.to_string();
        let card = Card::load(&mut conn, &household_id, &account_id.to_string()).await?;
        if cycle < card.starts {
            return Err(AppError::NotFound);
        }
        let rows = splits_for(&mut conn, &household_id, &card.facts.id, cycle, cycle).await?;
        let view = cycle_view(&mut conn, &household_id, &card, cycle, &entries(&rows)?).await?;
        let splits = rows
            .iter()
            .filter_map(|r| split_view(&card, cycle, r).transpose())
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok(CardCycleDetailView {
            cycle: view,
            splits,
        })
    }

    /// The current month of every card with a plan.
    pub async fn summaries(&self, ctx: &HouseholdCtx) -> Result<Vec<CardSummaryView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let household_id = ctx.household_id.to_string();
        let current = Cycle::of(today_in(&mut conn, ctx, &*self.clock).await?);
        let mut views = Vec::new();
        for plan in repo::plans(&mut conn, &household_id).await? {
            let card = Card::load(&mut conn, &household_id, &plan.account_id).await?;
            let cycle = current.max(card.starts);
            let rows = splits_for(&mut conn, &household_id, &card.facts.id, cycle, cycle).await?;
            let view = cycle_view(&mut conn, &household_id, &card, cycle, &entries(&rows)?).await?;
            views.push(CardSummaryView {
                account_id: card.facts.id.clone(),
                account_name: card.facts.name.clone(),
                current: view,
            });
        }
        Ok(views)
    }
}

/// The requested range, or the default; at most [`MAX_CYCLES`] months.
fn range(
    from: Option<Cycle>,
    to: Option<Cycle>,
    current: Cycle,
) -> Result<(Cycle, Cycle), AppError> {
    let to = to.unwrap_or(current);
    let from = match from {
        Some(from) => from,
        None => (1..DEFAULT_CYCLES).fold(to, |c, _| c.previous()),
    };
    if to < from {
        return Err(AppError::invalid("from", "must not be after to"));
    }
    let mut count = 1;
    let mut cycle = from;
    while cycle < to {
        cycle = cycle.next();
        count += 1;
        if count > MAX_CYCLES {
            return Err(AppError::invalid(
                "from",
                format!("at most {MAX_CYCLES} months at a time"),
            ));
        }
    }
    Ok((from, to))
}

/// A split as listed in `cycle`, or `None` when it belongs to another month.
fn split_view(
    card: &Card,
    cycle: Cycle,
    row: &CardSplitRow,
) -> Result<Option<CardSplitView>, AppError> {
    let own = Cycle::of(parse_stored(&row.date)?);
    let moved_in = row.moved && own.next() == cycle;
    let moved_out = row.moved && own == cycle;
    if own != cycle && !moved_in {
        return Ok(None);
    }
    Ok(Some(CardSplitView {
        split_id: row.id.clone(),
        group_id: row.group_id.clone(),
        date: row.date.clone(),
        description: row.description.clone(),
        counterparty: CounterpartyView {
            id: row.counterparty_id.clone(),
            name: row.counterparty_name.clone(),
        },
        amount: card.money(row.amount_minor),
        moved_in,
        moved_out,
    }))
}

/// One month's numbers. `entries` must cover the month and the one before.
pub(super) async fn cycle_view(
    conn: &mut SqliteConnection,
    household_id: &str,
    card: &Card,
    cycle: Cycle,
    entries: &[CardEntry],
) -> Result<CardCycleView, AppError> {
    let overflow = |e: saldo_core::MoneyError| AppError::Internal(e.to_string());
    let spent = card_cycle::cycle_spending(entries, cycle).map_err(overflow)?;
    let limit = card_cycle::limit_for(&card.limits, cycle)
        .ok_or_else(|| AppError::Internal(format!("no budget for {cycle}")))?;
    let available = card_cycle::available(limit, spent).map_err(overflow)?;
    let moved = |wanted: Cycle| -> i64 {
        entries
            .iter()
            .filter(|e| e.moved && Cycle::of(e.date) == wanted)
            .map(|e| e.amount)
            .sum()
    };
    let state = card.state(cycle)?;
    let group_id = card.rows.get(&cycle).and_then(|r| r.group_id.clone());
    let paid = match &group_id {
        Some(id) => paid_to_card(conn, household_id, id, &card.facts.id).await?,
        None => None,
    };
    let difference = match state {
        SettlementState::Synced => None,
        _ => Some(card.money(card_cycle::available(spent, paid.unwrap_or(0)).map_err(overflow)?)),
    };
    Ok(CardCycleView {
        cycle: cycle.to_string(),
        from: cycle.first_day().to_string(),
        to: cycle.last_day().to_string(),
        currency: card.facts.currency_code.clone(),
        limit: card.money(limit),
        spent: card.money(spent),
        available: card.money(available),
        moved_in: card.money(moved(cycle.previous())),
        moved_out: card.money(moved(cycle)),
        settlement: SettlementView {
            state: state.as_str().to_string(),
            date: cycle.settlement_date(card.day).to_string(),
            group_id: paid.and(group_id),
            amount: paid.map(|minor| card.money(minor)),
        },
        difference_after_settlement: difference,
    })
}

/// What a settlement group paid into the card, or `None` when the group is
/// gone.
async fn paid_to_card(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_id: &str,
    card_id: &str,
) -> Result<Option<i64>, AppError> {
    let splits = cycle_repo::settlement_splits(conn, household_id, group_id).await?;
    if splits.is_empty() {
        return Ok(None);
    }
    let paid = splits
        .iter()
        .filter(|s| s.destination_account_id == card_id)
        .map(|s| s.amount_minor)
        .sum();
    Ok(Some(paid))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(y: i32, m: u32) -> Cycle {
        Cycle::new(y, m).unwrap()
    }

    #[test]
    fn default_range_is_six_months_up_to_now() {
        assert_eq!(
            range(None, None, c(2026, 11)).unwrap(),
            (c(2026, 6), c(2026, 11))
        );
        assert_eq!(
            range(None, Some(c(2026, 2)), c(2026, 11)).unwrap(),
            (c(2025, 9), c(2026, 2))
        );
    }

    #[test]
    fn range_is_checked() {
        assert!(range(Some(c(2026, 5)), Some(c(2026, 4)), c(2026, 11)).is_err());
        assert!(range(Some(c(2023, 11)), Some(c(2026, 10)), c(2026, 11)).is_ok());
        assert!(range(Some(c(2023, 10)), Some(c(2026, 10)), c(2026, 11)).is_err());
    }
}
