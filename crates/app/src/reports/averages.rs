//! Average monthly spending per category over the last complete months.

use chrono::{Datelike, Months, NaiveDate};
use saldo_core::ratios::{change_bp, monthly_average};

use super::flows::category_rows;
use super::series::fold;
use super::view::{CategoryAverageItem, CategoryAveragesCurrency, CategoryAveragesReport};
use super::{Formatter, Interval, ReportQuery, ReportService};
use crate::ctx::HouseholdCtx;
use crate::error::AppError;
use crate::scope::Scope;

pub const MIN_AVERAGE_MONTHS: u32 = 3;
pub const MAX_AVERAGE_MONTHS: u32 = 24;
pub const DEFAULT_AVERAGE_MONTHS: u32 = 12;

impl ReportService {
    /// Per category: the average, smallest and largest monthly expenses over
    /// the `months` complete months before `today`'s, and this month so far.
    pub async fn category_averages(
        &self,
        ctx: &HouseholdCtx,
        today: NaiveDate,
        months: u32,
        scope: Scope,
    ) -> Result<CategoryAveragesReport, AppError> {
        if !(MIN_AVERAGE_MONTHS..=MAX_AVERAGE_MONTHS).contains(&months) {
            return Err(AppError::invalid(
                "months",
                format!("must be between {MIN_AVERAGE_MONTHS} and {MAX_AVERAGE_MONTHS}"),
            ));
        }
        let this_month = today.with_day(1).unwrap_or(today);
        let from = this_month
            .checked_sub_months(Months::new(months))
            .ok_or_else(|| AppError::invalid("months", "out of range"))?;
        let span = ReportQuery {
            from,
            to: today,
            interval: Interval::Month,
            scope,
        };
        let intervals = span.intervals()?;
        let mut conn = self.pool.acquire().await?;
        let rows = category_rows(&mut conn, ctx, &span).await?;
        let grouped = fold(&intervals, rows)?;
        let fmt = Formatter::load(&mut conn, ctx, grouped.keys()).await?;
        let mut currencies = Vec::new();
        for code in fmt.order(grouped.keys().cloned().collect()) {
            let Some(groups) = grouped.get(&code) else {
                continue;
            };
            let money = |minor| fmt.format(&code, minor);
            let mut stats: Vec<_> = groups
                .iter()
                .map(|(id, series)| {
                    let spent: Vec<i64> = series.intervals.iter().map(|&(_, e)| e).collect();
                    let (complete, current) = spent.split_at(spent.len().saturating_sub(1));
                    let average = monthly_average(complete);
                    let now = current.first().copied().unwrap_or(0);
                    (id, &series.name, complete.to_vec(), average, now)
                })
                .filter(|(_, _, complete, _, now)| *now > 0 || complete.iter().any(|&e| e > 0))
                .collect();
            stats.sort_by(|a, b| b.3.cmp(&a.3).then(a.1.cmp(b.1)));
            let mut items = Vec::with_capacity(stats.len());
            for (id, name, complete, average, now) in stats {
                items.push(CategoryAverageItem {
                    id: id.clone(),
                    name: name.clone(),
                    average: money(average)?,
                    min: money(complete.iter().copied().min().unwrap_or(0))?,
                    max: money(complete.iter().copied().max().unwrap_or(0))?,
                    this_month: money(now)?,
                    this_month_vs_average_bp: change_bp(now, average),
                });
            }
            if !items.is_empty() {
                currencies.push(CategoryAveragesCurrency {
                    currency: code.clone(),
                    items,
                });
            }
        }
        let last = this_month.pred_opt().unwrap_or(this_month);
        Ok(CategoryAveragesReport {
            months,
            from: from.to_string(),
            to: last.to_string(),
            currencies,
        })
    }
}
