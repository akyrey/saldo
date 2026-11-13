//! After the import: does every asset and liability account have the balance
//! Firefly reports? Compared on Firefly's `current_balance_date`, because
//! Firefly's balance ignores future-dated splits.

use chrono::NaiveDate;
use saldo_app::currencies;
use saldo_core::AccountId;

use crate::amounts::normalize;
use crate::engine::{ImportCx, ImportError};
use crate::entities::accounts::REF;
use crate::firefly::models::Account;
use crate::report::{BalanceLine, ImportReport};

pub async fn check(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError> {
    let accounts = cx
        .firefly
        .list::<Account>("/v1/accounts", &[("type", "all")])
        .await?;
    let mut guard = cx.state.lock().await;
    let state = &mut *guard;
    let places: std::collections::HashMap<String, u8> = currencies::all_in(&mut state.tx)
        .await?
        .into_iter()
        .map(|c| (c.code, c.decimal_places))
        .collect();
    for item in accounts {
        let a = item.attributes;
        if !matches!(
            a.account_type.as_str(),
            "asset" | "liabilities" | "liability"
        ) {
            continue;
        }
        let Some(local) = state
            .refs
            .resolve(REF, &item.id)
            .and_then(|id| id.parse::<AccountId>().ok())
        else {
            continue;
        };
        let date = a
            .current_balance_date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d.get(..10)?, "%Y-%m-%d").ok());
        let balance = cx
            .app
            .accounts
            .balance_in(&mut state.tx, &cx.ctx, local, date)
            .await?;
        let currency_places = places.get(&a.currency_code).copied().unwrap_or(2);
        let firefly = normalize(a.current_balance.as_deref().unwrap_or("0"), currency_places)
            .unwrap_or_else(|e| e);
        report.balances.push(BalanceLine {
            account: a.name,
            currency: a.currency_code,
            firefly,
            saldo: balance.balance,
        });
    }
    Ok(())
}
