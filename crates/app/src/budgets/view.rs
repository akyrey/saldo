//! The JSON shapes of budgets and limits, used by the API and in events, and
//! the conversions between them, stored rows and `saldo-core` types.

use chrono::NaiveDate;
use saldo_core::{AutoBudget, AutoBudgetType, DateRange, Limit, Money, Period, Spend};
use serde::Serialize;
use sqlx::SqliteConnection;

use super::repo::{BudgetRow, LimitRow, SpendRow};
use crate::currencies;
use crate::error::{AppError, parse_stored};

/// A budget as the API returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BudgetView {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub sort_order: i64,
    pub notes: Option<String>,
    /// `None` when the budget has no automatic limit.
    pub auto_budget: Option<AutoBudgetView>,
    pub created_at: String,
    pub updated_at: String,
}

/// An automatic limit: `amount` per `period`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct AutoBudgetView {
    /// `reset` or `rollover`.
    #[serde(rename = "type")]
    pub auto_type: String,
    /// `daily`, `weekly`, `monthly`, `quarterly`, `half_year` or `yearly`.
    pub period: String,
    pub amount: String,
    pub currency: String,
}

/// A budget limit with what was spent against it (withdrawals booked on the
/// budget, dated inside the limit, in its currency or with a foreign amount
/// in it) and what is left (negative when overspent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct BudgetLimitView {
    pub id: String,
    pub budget_id: String,
    pub start_date: String,
    pub end_date: String,
    pub amount: String,
    pub currency: String,
    pub spent: String,
    pub available: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) async fn budget_view(
    conn: &mut SqliteConnection,
    row: BudgetRow,
) -> Result<BudgetView, AppError> {
    let auto_budget = match (
        &row.auto_type,
        &row.auto_period,
        row.auto_amount_minor,
        &row.auto_currency_code,
    ) {
        (Some(auto_type), Some(period), Some(minor), Some(code)) => {
            let currency = currencies::find(conn, code, "auto_budget.currency").await?;
            Some(AutoBudgetView {
                auto_type: auto_type.clone(),
                period: period.clone(),
                amount: Money::new(minor, currency.code()).to_decimal_string(&currency),
                currency: code.clone(),
            })
        }
        _ => None,
    };
    Ok(BudgetView {
        id: row.id,
        name: row.name,
        active: row.active,
        sort_order: row.sort_order,
        notes: row.notes,
        auto_budget,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// The stored auto-budget as a `saldo-core` value; `None` when not set.
pub(crate) async fn core_auto_budget(
    conn: &mut SqliteConnection,
    row: &BudgetRow,
) -> Result<Option<AutoBudget>, AppError> {
    let (Some(auto_type), Some(period), Some(minor), Some(code)) = (
        &row.auto_type,
        &row.auto_period,
        row.auto_amount_minor,
        &row.auto_currency_code,
    ) else {
        return Ok(None);
    };
    let corrupt = |what: &str| AppError::Internal(format!("corrupt {what} on budget {}", row.id));
    let currency = currencies::find(conn, code, "auto_budget.currency").await?;
    Ok(Some(AutoBudget {
        kind: AutoBudgetType::parse(auto_type).ok_or_else(|| corrupt("auto_type"))?,
        period: Period::parse(period).ok_or_else(|| corrupt("auto_period"))?,
        amount: Money::new(minor, currency.code()),
    }))
}

pub(crate) fn core_limit(row: &LimitRow) -> Result<Limit, AppError> {
    let start: NaiveDate = parse_stored(&row.start_date)?;
    let end: NaiveDate = parse_stored(&row.end_date)?;
    let range = DateRange::new(start, end)
        .ok_or_else(|| AppError::Internal(format!("limit {} ends before it starts", row.id)))?;
    Ok(Limit {
        range,
        amount: Money::new(row.amount_minor, parse_stored(&row.currency_code)?),
    })
}

pub(crate) fn core_spends(rows: &[SpendRow]) -> Result<Vec<Spend>, AppError> {
    rows.iter()
        .map(|row| {
            let foreign_amount = match (row.foreign_amount_minor, &row.foreign_currency_code) {
                (Some(minor), Some(code)) => Some(Money::new(minor, parse_stored(code)?)),
                _ => None,
            };
            Ok(Spend {
                date: parse_stored(&row.date)?,
                amount: Money::new(row.amount_minor, parse_stored(&row.currency_code)?),
                foreign_amount,
            })
        })
        .collect()
}

/// The view of a limit, with spent and available computed by `saldo-core`
/// from `spends` (which must cover the limit's dates).
pub(crate) async fn limit_view(
    conn: &mut SqliteConnection,
    row: LimitRow,
    spends: &[Spend],
) -> Result<BudgetLimitView, AppError> {
    let limit = core_limit(&row)?;
    let currency = currencies::find(conn, &row.currency_code, "currency").await?;
    let overflow = |e: saldo_core::MoneyError| AppError::Internal(e.to_string());
    let spent = saldo_core::spent(&limit.range, currency.code(), spends).map_err(overflow)?;
    let available = saldo_core::available(&limit, spent).map_err(overflow)?;
    Ok(BudgetLimitView {
        id: row.id,
        budget_id: row.budget_id,
        start_date: row.start_date,
        end_date: row.end_date,
        amount: limit.amount.to_decimal_string(&currency),
        currency: row.currency_code,
        spent: spent.to_decimal_string(&currency),
        available: available.to_decimal_string(&currency),
        notes: row.notes,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
