//! Name → ID lookups for importing rules: Firefly rules refer to categories,
//! tags, bills, budgets, accounts and currencies by name.

use std::collections::HashMap;

use sqlx::SqliteConnection;

use crate::ctx::HouseholdCtx;
use crate::error::AppError;

/// Lowercased names (and currency names/codes) to IDs (codes) of one household.
#[derive(Debug, Clone, Default)]
pub struct RuleNames {
    pub categories: HashMap<String, String>,
    pub tags: HashMap<String, String>,
    pub bills: HashMap<String, String>,
    pub budgets: HashMap<String, String>,
    /// Asset and liability accounts win over others with the same name.
    pub accounts: HashMap<String, String>,
    /// Currency names and codes → code.
    pub currencies: HashMap<String, String>,
}

pub async fn names_in(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
) -> Result<RuleNames, AppError> {
    let household_id = ctx.household_id.to_string();
    let categories = sqlx::query!(
        "SELECT name, id FROM categories WHERE household_id = ?",
        household_id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| (r.name.to_lowercase(), r.id))
    .collect::<HashMap<_, _>>();
    let tags = sqlx::query!(
        "SELECT name, id FROM tags WHERE household_id = ?",
        household_id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| (r.name.to_lowercase(), r.id))
    .collect::<HashMap<_, _>>();
    let bills = sqlx::query!(
        "SELECT name, id FROM bills WHERE household_id = ?",
        household_id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| (r.name.to_lowercase(), r.id))
    .collect::<HashMap<_, _>>();
    let budgets = sqlx::query!(
        "SELECT name, id FROM budgets WHERE household_id = ?",
        household_id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| (r.name.to_lowercase(), r.id))
    .collect::<HashMap<_, _>>();
    // Ordered so that asset and liability accounts are inserted last and win.
    let accounts = sqlx::query!(
        "SELECT name, id FROM accounts WHERE household_id = ?
         ORDER BY CASE WHEN type IN ('asset', 'liability') THEN 1 ELSE 0 END",
        household_id
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|r| (r.name.to_lowercase(), r.id))
    .collect::<HashMap<_, _>>();
    let mut currencies = HashMap::new();
    for row in sqlx::query!("SELECT code, name FROM currencies")
        .fetch_all(&mut *conn)
        .await?
    {
        currencies.insert(row.name.to_lowercase(), row.code.clone());
        currencies.insert(row.code.to_lowercase(), row.code);
    }
    Ok(RuleNames {
        categories,
        tags,
        bills,
        budgets,
        accounts,
        currencies,
    })
}
