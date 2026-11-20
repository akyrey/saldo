//! Budget conditions: `budget:<limit id>:<80|100>` for each limit that
//! contains today and has reached a threshold.

use chrono::NaiveDate;
use saldo_core::Money;
use saldo_core::alerts::budget_thresholds;
use serde_json::json;
use sqlx::SqlitePool;

use super::Condition;
use crate::budgets::{BudgetFilter, BudgetLimitView, BudgetService};
use crate::ctx::HouseholdCtx;
use crate::currencies;
use crate::error::{AppError, parse_stored};
use crate::page::PageRequest;

/// A budget past 80% today, for the digest.
#[derive(Debug, Clone)]
pub(crate) struct BudgetOver {
    pub name: String,
    pub percent: i64,
}

/// The current limits' threshold conditions, plus the budgets over 80%.
pub(super) async fn conditions(
    pool: &SqlitePool,
    budgets: &BudgetService,
    ctx: &HouseholdCtx,
    today: NaiveDate,
) -> Result<(Vec<Condition>, Vec<BudgetOver>), AppError> {
    let filter = BudgetFilter { active: Some(true) };
    let all = budgets.list(ctx, &filter, PageRequest::max()).await?;
    let mut conditions = Vec::new();
    let mut over = Vec::new();
    for budget in all.items {
        let id = parse_stored(&budget.id)?;
        let limits = budgets
            .list_limits(ctx, id, Some(today), Some(today), PageRequest::max())
            .await?;
        for limit in limits.items {
            let (total, spent) = totals(pool, &limit).await?;
            let reached = budget_thresholds(total.minor(), spent.minor());
            if let Some(percent) =
                percent_of(spent.minor(), total.minor()).filter(|_| !reached.is_empty())
            {
                over.push(BudgetOver {
                    name: budget.name.clone(),
                    percent,
                });
            }
            let total_text = decimal(pool, total).await?;
            for threshold in reached {
                conditions.push(Condition {
                    key: format!("budget:{}:{threshold}", limit.id),
                    entity_type: "budget",
                    action: "threshold_reached",
                    entity_id: budget.id.clone(),
                    data: json!({
                        "budget": { "id": budget.id, "name": budget.name },
                        "limit": limit,
                        "threshold": threshold,
                        "total": total_text,
                    }),
                });
            }
        }
    }
    Ok((conditions, over))
}

/// What the limit allows (its amount plus what it carried in) and what was spent.
async fn totals(pool: &SqlitePool, limit: &BudgetLimitView) -> Result<(Money, Money), AppError> {
    let mut conn = pool.acquire().await?;
    let currency = currencies::find(&mut conn, &limit.currency, "currency").await?;
    let parse = |text: &str| {
        Money::parse_decimal(text, &currency)
            .map_err(|e| AppError::Internal(format!("bad stored amount {text:?}: {e}")))
    };
    let total = parse(&limit.amount)?
        .checked_add(parse(&limit.carried)?)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((total, parse(&limit.spent)?))
}

async fn decimal(pool: &SqlitePool, money: Money) -> Result<String, AppError> {
    let mut conn = pool.acquire().await?;
    let currency = currencies::find(&mut conn, &money.currency().to_string(), "currency").await?;
    Ok(money.to_decimal_string(&currency))
}

/// `spent` as a whole percentage of `total`, rounded down; `None` when
/// nothing could be spent.
fn percent_of(spent: i64, total: i64) -> Option<i64> {
    if total <= 0 {
        return None;
    }
    i64::try_from(i128::from(spent) * 100 / i128::from(total)).ok()
}
