//! Balances of many accounts at once, for the web app's account list.

use chrono::NaiveDate;
use saldo_core::Money;
use sqlx::SqliteConnection;

use super::repo::{self, AccountRow};
use super::{AccountFilter, AccountService, BalanceView, list_rows};
use crate::ctx::HouseholdCtx;
use crate::currencies;
use crate::error::AppError;
use crate::page::{Page, PageRequest};

impl AccountService {
    /// The balances of the accounts [`AccountService::list`] would return for
    /// the same filter and page, in the same order.
    pub async fn balances(
        &self,
        ctx: &HouseholdCtx,
        filter: &AccountFilter,
        date: Option<NaiveDate>,
        page: PageRequest,
    ) -> Result<Page<BalanceView>, AppError> {
        let mut conn = self.pool.acquire().await?;
        let (rows, total) = list_rows(&mut conn, ctx, filter, page).await?;
        let date = date.map(|d| d.to_string());
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(balance_view(&mut conn, row, date.as_deref()).await?);
        }
        Ok(Page::new(items, page, total))
    }
}

/// The balance of one account row up to and including `date`.
pub(super) async fn balance_view(
    conn: &mut SqliteConnection,
    row: AccountRow,
    date: Option<&str>,
) -> Result<BalanceView, AppError> {
    let minor = repo::balance_minor(conn, &row.household_id, &row, date).await?;
    let currency = currencies::find(conn, &row.currency_code, "currency").await?;
    Ok(BalanceView {
        account_id: row.id,
        date: date.map(str::to_string),
        balance: Money::new(minor, currency.code()).to_decimal_string(&currency),
        currency: row.currency_code,
    })
}
