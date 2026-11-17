//! Subscription suggestions: repeating payments found in past withdrawals
//! (`saldo_core::detect_subscriptions`), offered as bills. Suggestions are
//! computed on each request; accepting one creates the bill and links the
//! payments, dismissing one stores a "no" per counterparty and currency.

mod repo;

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Months, NaiveDate};
use saldo_core::{AccountId, BillId, GroupId, Money, Payment, SplitId, detect_subscriptions};
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::bills::{self, BillService, BillView, NewBill};
use crate::clock::{SharedClock, format_timestamp};
use crate::ctx::{HouseholdCtx, Origin};
use crate::currencies;
use crate::db::json_list;
use crate::error::{AppError, parse_stored};
use crate::events::{NewEvent, append_event};
use crate::identity::today_in;
use crate::scope::Scope;
use crate::transactions::{TransactionService, input_from_view};

/// How far back suggestions look: long enough for two yearly payments.
pub const LOOKBACK_MONTHS: u32 = 18;

/// A repeating payment that could become a bill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct SubscriptionCandidateView {
    /// The expense account paid.
    pub account_id: String,
    pub account_name: String,
    pub currency: String,
    /// `weekly`, `monthly`, `quarterly`, `half_year` or `yearly`.
    pub frequency: String,
    pub amount_min: String,
    pub amount_max: String,
    pub first_payment: String,
    pub last_payment: String,
    pub next_due: String,
    /// The matched payments, oldest first.
    pub split_ids: Vec<String>,
    /// The share of gaps that fit the frequency, 0 to 100.
    pub confidence: u8,
}

/// A suggestion the household does not want to see again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct SubscriptionDismissalView {
    pub account_id: String,
    pub currency: String,
    pub created_at: String,
}

/// Accepting a suggestion: the bill to create and the payments it covers.
#[derive(Debug, Clone)]
pub struct AcceptSubscription {
    pub account_id: AccountId,
    pub currency: String,
    pub name: String,
    pub frequency: String,
    pub amount_min: String,
    pub amount_max: String,
    pub split_ids: Vec<SplitId>,
}

#[derive(Clone)]
pub struct SubscriptionService {
    pool: SqlitePool,
    clock: SharedClock,
    bills: BillService,
    transactions: TransactionService,
}

impl SubscriptionService {
    pub fn new(
        pool: SqlitePool,
        clock: SharedClock,
        bills: BillService,
        transactions: TransactionService,
    ) -> Self {
        Self {
            pool,
            clock,
            bills,
            transactions,
        }
    }

    /// Repeating withdrawals of the last 18 months (paid from accounts in
    /// `scope`) that no bill covers and nobody dismissed.
    pub async fn candidates(
        &self,
        ctx: &HouseholdCtx,
        scope: Scope,
    ) -> Result<Vec<SubscriptionCandidateView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let today = today_in(&mut conn, ctx, &*self.clock).await?;
        let from = today
            .checked_sub_months(Months::new(LOOKBACK_MONTHS))
            .unwrap_or(NaiveDate::MIN);
        let household_id = ctx.household_id.to_string();
        let owner = scope.owner(ctx);
        let rows = repo::payments(
            &mut conn,
            &household_id,
            owner.as_deref(),
            &from.to_string(),
            &today.to_string(),
        )
        .await?;
        let names: HashMap<String, String> = rows
            .iter()
            .map(|r| (r.destination_account_id.clone(), r.destination_name.clone()))
            .collect();
        let payments = rows
            .iter()
            .map(|r| {
                Ok(Payment {
                    split_id: parse_stored(&r.id)?,
                    date: parse_stored(&r.date)?,
                    amount: Money::new(r.amount_minor, parse_stored(&r.currency_code)?),
                    counterparty: parse_stored(&r.destination_account_id)?,
                    has_bill: r.has_bill,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        let mut views = Vec::new();
        for candidate in detect_subscriptions(&payments, today) {
            let code = candidate.amount_min.currency().to_string();
            let currency = currencies::find(&mut conn, &code, "currency").await?;
            let account_id = candidate.counterparty.to_string();
            views.push(SubscriptionCandidateView {
                account_name: names.get(&account_id).cloned().unwrap_or_default(),
                account_id,
                currency: code,
                frequency: candidate.frequency.as_str().to_string(),
                amount_min: candidate.amount_min.to_decimal_string(&currency),
                amount_max: candidate.amount_max.to_decimal_string(&currency),
                first_payment: candidate.first_payment.to_string(),
                last_payment: candidate.last_payment.to_string(),
                next_due: candidate.next_due.to_string(),
                split_ids: candidate
                    .split_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect(),
                confidence: candidate.confidence,
            });
        }
        Ok(views)
    }

    /// Creates the bill (first due on the first payment) and links the
    /// payments to it, which pays their periods; the periods still open
    /// before the last payment are skipped, so the bill does not start
    /// overdue. One database transaction.
    pub async fn accept(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        input: AcceptSubscription,
    ) -> Result<BillView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let splits = checked_splits(&mut tx, ctx, &input).await?;
        let (Some(first), Some(last)) = (splits.first(), splits.last()) else {
            return Err(AppError::invalid(
                "split_ids",
                "must name at least one payment",
            ));
        };
        let (first_date, last_date) = (parse_stored(&first.date)?, parse_stored(&last.date)?);
        let bill = NewBill {
            name: input.name,
            amount_min: input.amount_min,
            amount_max: input.amount_max,
            currency: Some(input.currency),
            first_due_date: first_date,
            end_date: None,
            repeat_freq: input.frequency,
            skip: None,
            remind_days_before: None,
            remind_every_days: None,
            active: None,
            sort_order: None,
            notes: None,
        };
        let bill = self.bills.create_in(&mut tx, ctx, origin, bill).await?;
        let bill_id: BillId = parse_stored(&bill.id)?;
        // Split IDs per transaction group: a group is saved as a whole.
        let mut groups: BTreeMap<String, HashSet<String>> = BTreeMap::new();
        for split in &splits {
            groups
                .entry(split.group_id.clone())
                .or_default()
                .insert(split.id.clone());
        }
        for (group_id, ids) in groups {
            self.link_group(&mut tx, ctx, origin, &group_id, &ids, bill_id)
                .await?;
        }
        bills::skip_open_before_in(&mut tx, &*self.clock, ctx, origin, &bill.id, last_date).await?;
        let view = self.bills.get_in(&mut tx, ctx, bill_id).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Saves the group with `bill_id` on the splits in `ids` (no rules run:
    /// only the bill changes).
    async fn link_group(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        group_id: &str,
        ids: &HashSet<String>,
        bill_id: BillId,
    ) -> Result<(), AppError> {
        let group_id: GroupId = parse_stored(group_id)?;
        let view = self.transactions.get_in(conn, ctx, group_id).await?;
        let mut input = input_from_view(&view)?;
        for (split, target) in view.splits.iter().zip(input.splits.iter_mut()) {
            if ids.contains(&split.id) {
                target.bill_id = Some(bill_id);
            }
        }
        self.transactions
            .write_replace_in(conn, ctx, origin, group_id, input)
            .await?;
        Ok(())
    }

    /// Stops suggesting payments to `account_id` in `currency`
    /// (`subscription_dismissal.created`). Dismissing twice is a no-op.
    pub async fn dismiss(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        account_id: AccountId,
        currency: &str,
    ) -> Result<SubscriptionDismissalView, AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let account = account_id.to_string();
        if repo::account_type(&mut tx, &household_id, &account)
            .await?
            .as_deref()
            != Some("expense")
        {
            return Err(AppError::invalid(
                "account_id",
                "must be an expense account",
            ));
        }
        currencies::find(&mut tx, currency, "currency").await?;
        let now = format_timestamp(self.clock.now());
        let view = SubscriptionDismissalView {
            account_id: account,
            currency: currency.to_string(),
            created_at: now.clone(),
        };
        if repo::insert_dismissal(&mut tx, &household_id, &view.account_id, currency, &now).await? {
            self.emit(&mut tx, ctx, origin, &view, "created").await?;
        }
        tx.commit().await?;
        Ok(view)
    }

    /// Suggests payments to `account_id` in `currency` again
    /// (`subscription_dismissal.deleted`); 404 when not dismissed.
    pub async fn restore(
        &self,
        ctx: &HouseholdCtx,
        origin: Origin,
        account_id: AccountId,
        currency: &str,
    ) -> Result<(), AppError> {
        let mut tx = crate::db::begin(&self.pool).await?;
        let household_id = ctx.household_id.to_string();
        let account = account_id.to_string();
        if !repo::delete_dismissal(&mut tx, &household_id, &account, currency).await? {
            return Err(AppError::NotFound);
        }
        let view = SubscriptionDismissalView {
            account_id: account,
            currency: currency.to_string(),
            created_at: format_timestamp(self.clock.now()),
        };
        self.emit(&mut tx, ctx, origin, &view, "deleted").await?;
        tx.commit().await?;
        Ok(())
    }

    async fn emit(
        &self,
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        origin: Origin,
        view: &SubscriptionDismissalView,
        verb: &str,
    ) -> Result<(), AppError> {
        let id = format!("{}:{}", view.account_id, view.currency);
        let event = NewEvent::new(&*self.clock, "subscription_dismissal", verb, id, view)?;
        append_event(conn, ctx, origin, event).await?;
        Ok(())
    }
}

/// The named splits, oldest first, after checking each is a withdrawal to
/// the account in the currency that no bill covers yet.
async fn checked_splits(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    input: &AcceptSubscription,
) -> Result<Vec<repo::NamedSplitRow>, AppError> {
    let ids: HashSet<String> = input.split_ids.iter().map(|id| id.to_string()).collect();
    let json = json_list(&ids.iter().cloned().collect::<Vec<_>>())?;
    let rows = repo::named_splits(conn, &ctx.household_id.to_string(), &json).await?;
    if rows.len() != ids.len() {
        return Err(AppError::invalid(
            "split_ids",
            "names a split that does not exist",
        ));
    }
    let account = input.account_id.to_string();
    let fits = |r: &repo::NamedSplitRow| {
        r.kind == "withdrawal"
            && r.destination_account_id == account
            && r.currency_code == input.currency
            && r.bill_id.is_none()
    };
    if let Some(bad) = rows.iter().find(|r| !fits(r)) {
        return Err(AppError::invalid(
            "split_ids",
            format!(
                "split {} must be a withdrawal to the account in {} without a bill",
                bad.id, input.currency
            ),
        ));
    }
    Ok(rows)
}

/// Every dismissal of the household, for the export.
pub(crate) async fn dismissals_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<Vec<SubscriptionDismissalView>, AppError> {
    let rows = repo::dismissals(conn, &ctx.household_id.to_string()).await?;
    Ok(rows
        .into_iter()
        .map(
            |(account_id, currency, created_at)| SubscriptionDismissalView {
                account_id,
                currency,
                created_at,
            },
        )
        .collect())
}
