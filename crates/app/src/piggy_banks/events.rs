//! Adding money to and removing it from a piggy bank.

use chrono::NaiveDate;
use saldo_core::{
    AccountId, Currency, EventOutcome, Money, PiggyBankId, PiggyBankState, PiggyEventError,
    SplitId, check_event, check_imported_event, room_to_add,
};
use serde::Serialize;
use sqlx::SqliteConnection;

use super::repo::{self, EventRow, PiggyBankRow};
use super::view::{decimal, view_of};
use super::{ENTITY, PiggyBankService, find};
use crate::accounts::repo as account_repo;
use crate::clock::{Clock, format_timestamp, new_id};
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::error::AppError;
use crate::events::{NewEvent, append_event};
use crate::page::{Page, PageRequest};
use crate::transactions::SplitView;

/// Money added (positive) or removed (negative), optionally tied to the
/// transfer split that moved it.
#[derive(Debug, Clone)]
pub struct NewPiggyBankEvent {
    /// Required when more than one account is linked.
    pub account_id: Option<AccountId>,
    /// Signed decimal string, for example `"25.00"` or `"-10.00"`.
    pub amount: String,
    pub date: NaiveDate,
    pub split_id: Option<SplitId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct PiggyBankEventView {
    pub id: String,
    pub piggy_bank_id: String,
    pub account_id: String,
    /// Signed: positive added, negative removed.
    pub amount: String,
    pub currency: String,
    pub split_id: Option<String>,
    pub date: String,
    pub created_at: String,
}

impl PiggyBankService {
    pub async fn add_event(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: PiggyBankId,
        input: NewPiggyBankEvent,
    ) -> Result<PiggyBankEventView, AppError> {
        let mut tx = self.pool.begin().await?;
        let result = self.add_event_in(&mut tx, ctx, origin, id, input).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Like [`Self::add_event`], inside the caller's transaction. Imports
    /// skip the balance and target checks (history recorded elsewhere).
    pub async fn add_event_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        id: PiggyBankId,
        input: NewPiggyBankEvent,
    ) -> Result<PiggyBankEventView, AppError> {
        let row = find(&mut *conn, ctx, id).await?;
        let currency = currencies::find(&mut *conn, &row.currency_code, "currency").await?;
        let amount = Money::parse_decimal(&input.amount, &currency)
            .map_err(|e| AppError::invalid("amount", e.to_string()))?
            .minor();
        let account_id = pick_account(&mut *conn, &row, input.account_id).await?;
        if let Some(split_id) = input.split_id {
            check_split(&mut *conn, &row.household_id, split_id, &account_id).await?;
        }
        let entry = Entry {
            account_id,
            amount,
            split_id: input.split_id.map(|s| s.to_string()),
            date: input.date.to_string(),
        };
        record(&mut *conn, &*self.clock, ctx, origin, row, &currency, entry).await
    }

    /// The linked account a split moves money into or out of (the
    /// destination first), if any: which account an imported event belongs to.
    pub async fn linked_side_of_split_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: PiggyBankId,
        split_id: SplitId,
    ) -> Result<Option<AccountId>, AppError> {
        let row = find(&mut *conn, ctx, id).await?;
        let Some((_, source, destination)) =
            repo::split_sides(&mut *conn, &row.household_id, &split_id.to_string()).await?
        else {
            return Ok(None);
        };
        let linked = repo::accounts(&mut *conn, &row.household_id, &row.id).await?;
        let side = [destination, source]
            .into_iter()
            .find(|side| linked.iter().any(|(id, _)| id == side));
        side.map(|id| crate::error::parse_stored(&id)).transpose()
    }

    /// A piggy bank's events, newest first.
    pub async fn list_events(
        &self,
        ctx: &HouseholdCtx,
        id: PiggyBankId,
        page: PageRequest,
    ) -> Result<Page<PiggyBankEventView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        self.list_events_in(&mut conn, ctx, id, page).await
    }

    /// Like [`Self::list_events`], inside the caller's transaction.
    pub async fn list_events_in(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        id: PiggyBankId,
        page: PageRequest,
    ) -> Result<Page<PiggyBankEventView>, AppError> {
        let row = find(&mut *conn, ctx, id).await?;
        let currency = currencies::find(&mut *conn, &row.currency_code, "currency").await?;
        let (rows, total) = repo::list_events(
            &mut *conn,
            &row.household_id,
            &row.id,
            page.limit(),
            page.offset(),
        )
        .await?;
        let items = rows.into_iter().map(|e| event_view(e, &currency)).collect();
        Ok(Page::new(items, page, total))
    }
}

/// Books the transfer `split` (just created from a recurrence template) on
/// the piggy bank: money into a linked account is added, money out of one
/// removed. The amount is capped to what the rules allow; a split that
/// touches no linked account, is no transfer, or is in another currency adds
/// nothing (`Ok(None)`).
pub(crate) async fn add_for_split_in(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    ctx: &HouseholdCtx,
    origin: Origin,
    id: PiggyBankId,
    split: &SplitView,
) -> Result<Option<PiggyBankEventView>, AppError> {
    let row = find(&mut *conn, ctx, id).await?;
    if split.kind != "transfer" || split.currency != row.currency_code {
        return Ok(None);
    }
    let currency = currencies::find(&mut *conn, &row.currency_code, "currency").await?;
    let linked = repo::accounts(&mut *conn, &row.household_id, &row.id).await?;
    let is_linked = |id: &str| linked.iter().any(|(linked, _)| linked == id);
    let minor = Money::parse_decimal(&split.amount, &currency)
        .map_err(|e| AppError::Internal(format!("stored split amount: {e}")))?
        .minor();
    let (account_id, wanted) = if is_linked(&split.destination_account_id) {
        (split.destination_account_id.clone(), minor)
    } else if is_linked(&split.source_account_id) {
        (split.source_account_id.clone(), -minor)
    } else {
        return Ok(None);
    };
    let state = load_state(&mut *conn, &row, &account_id).await?;
    let amount = if wanted > 0 {
        wanted.min(room_to_add(state))
    } else {
        wanted.max(-state.saved_in_account.max(0))
    };
    if amount == 0 {
        return Ok(None);
    }
    let entry = Entry {
        account_id,
        amount,
        split_id: Some(split.id.clone()),
        date: split.date.clone(),
    };
    record(conn, clock, ctx, origin, row, &currency, entry)
        .await
        .map(Some)
}

/// A checked-to-be-linked event about to be written.
struct Entry {
    account_id: String,
    amount: i64,
    split_id: Option<String>,
    date: String,
}

/// Checks the rules, writes the event and appends `piggy_bank.updated` (and
/// `piggy_bank.target_reached` when the target was just reached).
async fn record(
    conn: &mut SqliteConnection,
    clock: &dyn Clock,
    ctx: &HouseholdCtx,
    origin: Origin,
    row: PiggyBankRow,
    currency: &Currency,
    entry: Entry,
) -> Result<PiggyBankEventView, AppError> {
    let state = load_state(&mut *conn, &row, &entry.account_id).await?;
    let checked = if origin == Origin::Import {
        check_imported_event(state, entry.amount)
    } else {
        check_event(state, entry.amount)
    };
    let outcome: EventOutcome =
        checked.map_err(|e| AppError::invalid("amount", explain(e, currency)))?;
    let event = EventRow {
        id: new_id(clock).to_string(),
        household_id: row.household_id.clone(),
        piggy_bank_id: row.id.clone(),
        account_id: entry.account_id,
        amount_minor: entry.amount,
        split_id: entry.split_id,
        date: entry.date,
        created_at: format_timestamp(clock.now()),
    };
    repo::insert_event(&mut *conn, &event).await?;
    let view = view_of(&mut *conn, row).await?;
    let updated = NewEvent::new(clock, ENTITY, "updated", view.id.clone(), &view)?;
    append_event(&mut *conn, ctx, origin, updated).await?;
    if outcome.target_reached {
        let reached = NewEvent::new(clock, ENTITY, "target_reached", view.id.clone(), &view)?;
        append_event(&mut *conn, ctx, origin, reached).await?;
    }
    Ok(event_view(event, currency))
}

async fn load_state(
    conn: &mut SqliteConnection,
    row: &PiggyBankRow,
    account_id: &str,
) -> Result<PiggyBankState, AppError> {
    let (saved_total, saved_in_account) = repo::saved(&mut *conn, &row.id, account_id).await?;
    let account = account_repo::get(&mut *conn, &row.household_id, account_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let account_balance =
        account_repo::balance_minor(&mut *conn, &row.household_id, &account, None).await?;
    Ok(PiggyBankState {
        saved_total,
        saved_in_account,
        account_balance,
        target: row.target_amount_minor,
    })
}

/// The account to book on: the one given, which must be linked, or the only
/// linked one.
async fn pick_account(
    conn: &mut SqliteConnection,
    row: &PiggyBankRow,
    given: Option<AccountId>,
) -> Result<String, AppError> {
    let linked = repo::accounts(&mut *conn, &row.household_id, &row.id).await?;
    match (given, linked.as_slice()) {
        (Some(id), _) => {
            let id = id.to_string();
            if linked.iter().any(|(linked, _)| *linked == id) {
                Ok(id)
            } else {
                Err(AppError::invalid(
                    "account_id",
                    "is not linked to this piggy bank",
                ))
            }
        }
        (None, [(only, _)]) => Ok(only.clone()),
        (None, _) => Err(AppError::invalid(
            "account_id",
            "is required when several accounts are linked",
        )),
    }
}

/// The split must be a transfer in the household that moves money in or
/// out of `account_id`.
async fn check_split(
    conn: &mut SqliteConnection,
    household_id: &str,
    split_id: SplitId,
    account_id: &str,
) -> Result<(), AppError> {
    let field = "split_id";
    let (kind, source, destination) =
        repo::split_sides(&mut *conn, household_id, &split_id.to_string())
            .await?
            .ok_or_else(|| AppError::invalid(field, "unknown split"))?;
    if kind != "transfer" {
        return Err(AppError::invalid(field, "must be a transfer"));
    }
    if source != account_id && destination != account_id {
        return Err(AppError::invalid(
            field,
            "does not move money in or out of the account",
        ));
    }
    Ok(())
}

fn explain(error: PiggyEventError, currency: &Currency) -> String {
    let amount = |minor: i64| format!("{} {}", decimal(minor, currency), currency.code());
    match error {
        PiggyEventError::ZeroAmount => "must not be zero".to_string(),
        PiggyEventError::BelowZero { saved } => {
            format!(
                "removes more than is saved in the account ({})",
                amount(saved)
            )
        }
        PiggyEventError::ExceedsTarget { room } => {
            format!("goes past the target ({} left to save)", amount(room))
        }
        PiggyEventError::ExceedsBalance { available } => format!(
            "exceeds the account balance not yet saved ({})",
            amount(available)
        ),
        PiggyEventError::Overflow => "is too large".to_string(),
    }
}

fn event_view(row: EventRow, currency: &Currency) -> PiggyBankEventView {
    PiggyBankEventView {
        id: row.id,
        piggy_bank_id: row.piggy_bank_id,
        account_id: row.account_id,
        amount: decimal(row.amount_minor, currency),
        currency: currency.code().to_string(),
        split_id: row.split_id,
        date: row.date,
        created_at: row.created_at,
    }
}
