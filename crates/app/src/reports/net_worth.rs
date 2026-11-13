//! Net worth over time: the balances of the asset and liability accounts
//! marked `include_net_worth`, summed per account currency.

use std::collections::BTreeMap;

use sqlx::SqliteConnection;

use super::series::interval_of;
use super::view::*;
use super::{Formatter, ReportQuery, ReportService, parse_day};
use crate::ctx::HouseholdCtx;
use crate::error::AppError;

/// Per currency: the balance before `from`, and the change per interval.
type Changes = BTreeMap<String, (i64, Vec<i64>)>;

impl ReportService {
    /// Net worth at the end of each interval. The scope filters by account
    /// owner (joint accounts only count for the whole household).
    pub async fn net_worth(
        &self,
        ctx: &HouseholdCtx,
        query: &ReportQuery,
    ) -> Result<NetWorthReport, AppError> {
        let intervals = query.intervals()?;
        let mut conn = self.pool.acquire().await?;
        let changes = changes(&mut conn, ctx, query, &intervals).await?;
        let fmt = Formatter::load(&mut conn, ctx, changes.keys()).await?;
        let overflow = || AppError::Internal("net worth too large".to_string());
        let mut currencies = Vec::new();
        for code in fmt.order(changes.keys().cloned().collect()) {
            let Some((opening, deltas)) = changes.get(&code) else {
                continue;
            };
            let mut value = *opening;
            let mut rows = Vec::with_capacity(intervals.len());
            for (range, &change) in intervals.iter().zip(deltas) {
                value = value.checked_add(change).ok_or_else(overflow)?;
                rows.push(NetWorthInterval {
                    start: range.start.to_string(),
                    end: range.end.to_string(),
                    value: fmt.format(&code, value)?,
                    change: fmt.format(&code, change)?,
                });
            }
            currencies.push(NetWorthCurrency {
                opening: fmt.format(&code, *opening)?,
                intervals: rows,
                currency: code.clone(),
            });
        }
        Ok(NetWorthReport {
            range: query.range(),
            currencies,
        })
    }
}

async fn changes(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    query: &ReportQuery,
    intervals: &[saldo_core::DateRange],
) -> Result<Changes, AppError> {
    let (household_id, owner) = (ctx.household_id.to_string(), query.scope.owner(ctx));
    let (from, to) = (query.from.to_string(), query.to.to_string());
    // One row per account side of a split: + when the account receives,
    // - when it pays, in the account's currency (the foreign amount when the
    // account is in the split's foreign currency). Days before `from` are
    // summed into one row with an empty date.
    let rows = sqlx::query!(
        r#"SELECT CASE WHEN s.date < ?2 THEN '' ELSE s.date END AS "day!: String",
                  a.currency_code AS "currency!",
                  SUM(CASE WHEN s.destination_account_id = a.id THEN 1 ELSE -1 END
                      * CASE WHEN s.foreign_currency_code = a.currency_code
                             THEN s.foreign_amount_minor ELSE s.amount_minor END) AS "total!: i64"
           FROM splits s
           JOIN accounts a ON a.household_id = s.household_id
                          AND a.id IN (s.source_account_id, s.destination_account_id)
           WHERE s.household_id = ?1 AND s.date <= ?3
             AND a.type IN ('asset', 'liability') AND a.include_net_worth = 1
             AND (?4 IS NULL OR a.owner_user_id = ?4)
           GROUP BY 1, a.currency_code"#,
        household_id,
        from,
        to,
        owner,
    )
    .fetch_all(&mut *conn)
    .await?;
    let overflow = || AppError::Internal("net worth too large".to_string());
    let mut changes = Changes::new();
    for row in rows {
        let entry = changes
            .entry(row.currency)
            .or_insert_with(|| (0, vec![0; intervals.len()]));
        let slot = if row.day.is_empty() {
            &mut entry.0
        } else {
            match interval_of(intervals, parse_day(&row.day)?) {
                Some(index) => &mut entry.1[index],
                None => continue,
            }
        };
        *slot = slot.checked_add(row.total).ok_or_else(overflow)?;
    }
    Ok(changes)
}
