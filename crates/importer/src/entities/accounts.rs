//! `/v1/accounts` → `accounts` (ARCHITECTURE.md 9.1, row 2).

use std::collections::HashMap;

use saldo_app::accounts::{AccountFilter, AccountUpdate, AccountView, NewAccount};
use saldo_app::ctx::Origin;
use saldo_app::currencies;
use saldo_app::page::PageRequest;
use saldo_core::AccountId;

use crate::amounts::normalize;
use crate::engine::{ImportCx, ImportError, State};
use crate::firefly::models::{Account, Resource};
use crate::report::ImportReport;

const ENTITY: &str = "accounts";
/// Reference type of an account; transactions resolve their accounts by it.
pub const REF: &str = "account";
/// A Firefly cash account becomes two Saldo accounts named `(cash)`.
pub const CASH_EXPENSE: &str = "account.cash_expense";
pub const CASH_REVENUE: &str = "account.cash_revenue";
const CASH_NAME: &str = "(cash)";
const MAPPED_NOTE: &str =
    "Firefly initial-balance/reconciliation accounts mapped to Saldo's equity accounts";

pub async fn import_accounts(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError> {
    let accounts = cx
        .firefly
        .list::<Account>("/v1/accounts", &[("type", "all")])
        .await?;
    let mut guard = cx.state.lock().await;
    let state = &mut *guard;
    let equity = equity_accounts(cx, state).await?;
    let places: HashMap<String, u8> = currencies::all_in(&mut state.tx)
        .await?
        .into_iter()
        .map(|c| (c.code, c.decimal_places))
        .collect();

    for item in &accounts {
        let a = &item.attributes;
        match a.account_type.as_str() {
            "initial-balance" | "reconciliation" => {
                let role = if a.account_type == "reconciliation" {
                    "reconciliation"
                } else {
                    "opening_balance"
                };
                let local = equity.get(role).ok_or_else(|| {
                    ImportError::Invalid(format!("household has no {role} equity account"))
                })?;
                state
                    .refs
                    .upsert(&mut state.tx, REF, &item.id, local, &cx.now())
                    .await?;
                report.unchanged(ENTITY);
                report.note(MAPPED_NOTE);
            }
            "cash" => import_cash(cx, state, report, item).await?,
            "import" => report.skipped(
                ENTITY,
                format!("import account {:?} is not used by Saldo", a.name),
            ),
            other => match saldo_type(other) {
                Some(account_type) => {
                    upsert(cx, state, report, item, account_type, &places).await?
                }
                None => report.skipped(ENTITY, format!("{:?} has unknown type {other:?}", a.name)),
            },
        }
    }
    Ok(())
}

/// Firefly account type → Saldo account type.
fn saldo_type(firefly: &str) -> Option<&'static str> {
    match firefly {
        "asset" => Some("asset"),
        "expense" => Some("expense"),
        "revenue" => Some("revenue"),
        "liabilities" | "liability" | "loan" | "debt" | "mortgage" => Some("liability"),
        _ => None,
    }
}

fn asset_role(firefly: &str) -> Option<String> {
    let role = match firefly {
        "defaultAsset" => "default",
        "sharedAsset" => "shared",
        "savingAsset" => "savings",
        "ccAsset" => "credit_card",
        "cashWalletAsset" => "cash_wallet",
        _ => return None,
    };
    Some(role.to_string())
}

fn liability_direction(firefly: &str) -> Option<String> {
    match firefly {
        "debit" => Some("i_owe".to_string()),
        "credit" => Some("owed_to_me".to_string()),
        _ => None,
    }
}

fn interest_period(firefly: &str) -> Option<String> {
    match firefly {
        "daily" | "weekly" | "monthly" | "quarterly" | "yearly" => Some(firefly.to_string()),
        "half-year" | "half_year" => Some("half_year".to_string()),
        _ => None,
    }
}

/// `Some(text)` unless it is empty after trimming.
fn text(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// The Saldo account a Firefly account should become.
fn desired(
    a: &Account,
    account_type: &str,
    places: &HashMap<String, u8>,
) -> Result<NewAccount, String> {
    let liability = account_type == "liability";
    let currency_places = *places
        .get(&a.currency_code)
        .ok_or_else(|| format!("unknown currency {}", a.currency_code))?;
    let virtual_balance = normalize(a.virtual_balance.as_deref().unwrap_or("0"), currency_places)?;
    let interest = text(&a.interest)
        .filter(|_| liability)
        .map(|rate| normalize(&rate, 8))
        .transpose()?;
    Ok(NewAccount {
        name: a.name.trim().to_string(),
        account_type: account_type.to_string(),
        owner_user_id: None,
        currency: Some(a.currency_code.clone()),
        asset_role: a
            .account_role
            .as_deref()
            .filter(|_| account_type == "asset")
            .and_then(asset_role),
        liability_type: text(&a.liability_type).filter(|_| liability),
        liability_direction: a
            .liability_direction
            .as_deref()
            .filter(|_| liability)
            .and_then(liability_direction),
        interest_rate: interest.map(|rate| trim_zeros(&rate)),
        interest_period: a
            .interest_period
            .as_deref()
            .filter(|_| liability)
            .and_then(interest_period),
        iban: text(&a.iban),
        bic: text(&a.bic),
        account_number: text(&a.account_number),
        virtual_balance: Some(virtual_balance),
        include_net_worth: Some(a.include_net_worth.unwrap_or(true)),
        active: Some(a.active),
        sort_order: None,
        notes: text(&a.notes),
    })
}

/// `"3.50000000"` → `"3.5"`, `"4.00000000"` → `"4"`.
fn trim_zeros(rate: &str) -> String {
    if !rate.contains('.') {
        return rate.to_string();
    }
    rate.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// True if the stored account already looks like `desired`.
fn same(view: &AccountView, d: &NewAccount) -> bool {
    view.name == d.name
        && view.asset_role == d.asset_role
        && view.liability_type == d.liability_type
        && view.liability_direction == d.liability_direction
        && view.interest_rate == d.interest_rate
        && view.interest_period == d.interest_period
        && view.iban == d.iban
        && view.bic == d.bic
        && view.account_number == d.account_number
        && Some(&view.virtual_balance) == d.virtual_balance.as_ref()
        && Some(view.include_net_worth) == d.include_net_worth
        && Some(view.active) == d.active
        && view.notes == d.notes
}

fn update_from(d: NewAccount) -> AccountUpdate {
    AccountUpdate {
        name: Some(d.name),
        account_type: Some(d.account_type),
        owner_user_id: None,
        asset_role: Some(d.asset_role),
        liability_type: Some(d.liability_type),
        liability_direction: Some(d.liability_direction),
        interest_rate: Some(d.interest_rate),
        interest_period: Some(d.interest_period),
        iban: Some(d.iban),
        bic: Some(d.bic),
        account_number: Some(d.account_number),
        virtual_balance: d.virtual_balance,
        include_net_worth: d.include_net_worth,
        active: d.active,
        sort_order: None,
        notes: Some(d.notes),
    }
}

async fn upsert(
    cx: &ImportCx,
    state: &mut State,
    report: &mut ImportReport,
    item: &Resource<Account>,
    account_type: &str,
    places: &HashMap<String, u8>,
) -> Result<(), ImportError> {
    let name = item.attributes.name.trim().to_string();
    let wanted = match desired(&item.attributes, account_type, places) {
        Ok(wanted) => wanted,
        Err(why) => {
            report.skipped(ENTITY, format!("{name:?}: {why}"));
            return Ok(());
        }
    };
    let service = &cx.app.accounts;
    let existing = state
        .refs
        .resolve(REF, &item.id)
        .and_then(|id| id.parse::<AccountId>().ok());
    let current = match existing {
        Some(id) => service.get_in(&mut state.tx, &cx.ctx, id).await.ok(),
        None => None,
    };
    let result = match current {
        Some(view)
            if view.account_type != account_type
                || Some(&view.currency) != wanted.currency.as_ref() =>
        {
            report.skipped(
                ENTITY,
                format!("{name:?}: type or currency changed in Firefly; Saldo keeps them fixed"),
            );
            return Ok(());
        }
        Some(view) if same(&view, &wanted) => {
            report.unchanged(ENTITY);
            return Ok(());
        }
        Some(view) => {
            let id = account_id(&view.id)?;
            service
                .update_in(
                    &mut state.tx,
                    &cx.ctx,
                    Origin::Import,
                    id,
                    update_from(wanted),
                )
                .await
                .map(|v| (v.id, false))
        }
        None => service
            .create_in(&mut state.tx, &cx.ctx, Origin::Import, wanted)
            .await
            .map(|v| (v.id, true)),
    };
    match result {
        Ok((local_id, created)) => {
            state
                .refs
                .upsert(&mut state.tx, REF, &item.id, &local_id, &cx.now())
                .await?;
            if created {
                report.created(ENTITY)
            } else {
                report.updated(ENTITY)
            }
        }
        Err(error) => report.skipped(ENTITY, format!("{name:?}: {}", super::skippable(error)?)),
    }
    Ok(())
}

async fn import_cash(
    cx: &ImportCx,
    state: &mut State,
    report: &mut ImportReport,
    item: &Resource<Account>,
) -> Result<(), ImportError> {
    for (reference, account_type) in [(CASH_EXPENSE, "expense"), (CASH_REVENUE, "revenue")] {
        if state.refs.resolve(reference, &item.id).is_some() {
            report.unchanged(ENTITY);
            continue;
        }
        let input = NewAccount {
            name: CASH_NAME.to_string(),
            account_type: account_type.to_string(),
            currency: Some(item.attributes.currency_code.clone()),
            ..NewAccount::default()
        };
        match cx
            .app
            .accounts
            .create_in(&mut state.tx, &cx.ctx, Origin::Import, input)
            .await
        {
            Ok(view) => {
                state
                    .refs
                    .upsert(&mut state.tx, reference, &item.id, &view.id, &cx.now())
                    .await?;
                report.created(ENTITY);
            }
            Err(error) => report.skipped(
                ENTITY,
                format!("(cash) {account_type}: {}", super::skippable(error)?),
            ),
        }
    }
    Ok(())
}

/// The household's equity accounts by role (`opening_balance`, `reconciliation`).
async fn equity_accounts(
    cx: &ImportCx,
    state: &mut State,
) -> Result<HashMap<String, String>, ImportError> {
    let filter = AccountFilter {
        account_type: Some("equity".to_string()),
        ..AccountFilter::default()
    };
    let page = cx
        .app
        .accounts
        .list_in(&mut state.tx, &cx.ctx, &filter, PageRequest::default())
        .await?;
    Ok(page
        .items
        .into_iter()
        .filter_map(|a| a.equity_role.map(|role| (role, a.id)))
        .collect())
}

fn account_id(text: &str) -> Result<AccountId, ImportError> {
    text.parse()
        .map_err(|_| ImportError::Invalid(format!("bad account id {text}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_types_roles_directions_and_periods() {
        assert_eq!(saldo_type("liabilities"), Some("liability"));
        assert_eq!(saldo_type("cash"), None);
        assert_eq!(asset_role("ccAsset").as_deref(), Some("credit_card"));
        assert_eq!(asset_role("unknown"), None);
        assert_eq!(liability_direction("debit").as_deref(), Some("i_owe"));
        assert_eq!(liability_direction("credit").as_deref(), Some("owed_to_me"));
        assert_eq!(interest_period("half-year").as_deref(), Some("half_year"));
        assert_eq!(interest_period("fortnightly"), None);
        assert_eq!(trim_zeros("3.50000000"), "3.5");
        assert_eq!(trim_zeros("4.00000000"), "4");
        assert_eq!(text(&Some("  ".into())), None);
    }
}
