//! `/v1/piggy-banks` → `piggy_banks` + `piggy_bank_accounts`, and (after
//! the transactions, so transfer journals resolve) their events →
//! `piggy_bank_events`.

use std::collections::{BTreeSet, HashMap};

use chrono::NaiveDate;
use saldo_app::ctx::Origin;
use saldo_app::currencies;
use saldo_app::piggy_banks::{NewPiggyBank, NewPiggyBankEvent, PiggyBankUpdate, PiggyBankView};
use saldo_core::{AccountId, PiggyBankId, SplitId};

use super::accounts::REF as ACCOUNT_REF;
use crate::amounts::normalize;
use crate::engine::{ImportCx, ImportError, State};
use crate::firefly::planning_models::{PiggyBank, PiggyBankEvent};
use crate::report::ImportReport;

const ENTITY: &str = "piggy_banks";
const EVENTS: &str = "piggy_bank_events";
pub const REF: &str = "piggy_bank";
const EVENT_REF: &str = "piggy_bank_event";
const SPLIT_REF: &str = "split";

/// The date part of a Firefly date-time (`2026-10-01T00:00:00+00:00`).
pub(crate) fn date_part(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

/// Decimal places per currency code, including currencies imported earlier
/// in this run.
pub(crate) async fn places(state: &mut State) -> Result<HashMap<String, u8>, ImportError> {
    Ok(currencies::all_in(&mut state.tx)
        .await?
        .into_iter()
        .map(|c| (c.code, c.decimal_places))
        .collect())
}

pub async fn import_piggy_banks(
    cx: &ImportCx,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    let items = cx.firefly.list::<PiggyBank>("/v1/piggy-banks", &[]).await?;
    let mut guard = cx.state.lock().await;
    let state = &mut *guard;
    let places = places(state).await?;
    for item in items {
        let input = match to_input(state, &places, &item.attributes) {
            Ok(input) => input,
            Err(why) => {
                report.skipped(ENTITY, format!("{:?}: {why}", item.attributes.name));
                continue;
            }
        };
        let existing = state
            .refs
            .resolve(REF, &item.id)
            .and_then(|id| id.parse::<PiggyBankId>().ok());
        let current = match existing {
            Some(id) => cx
                .app
                .piggy_banks
                .get_in(&mut state.tx, &cx.ctx, id)
                .await
                .ok(),
            None => None,
        };
        let service = &cx.app.piggy_banks;
        let result = match current {
            Some(view) if same(&view, &input) => {
                report.unchanged(ENTITY);
                continue;
            }
            Some(view) => {
                let id = view
                    .id
                    .parse()
                    .map_err(|_| ImportError::Invalid(format!("bad id {}", view.id)))?;
                service
                    .update_in(&mut state.tx, &cx.ctx, Origin::Import, id, as_update(input))
                    .await
                    .map(|v| (v.id, false))
            }
            None => service
                .create_in(&mut state.tx, &cx.ctx, Origin::Import, input)
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
                    report.created(ENTITY);
                } else {
                    report.updated(ENTITY);
                }
            }
            Err(error) => {
                let why = super::skippable(error)?;
                report.skipped(ENTITY, format!("{:?}: {why}", item.attributes.name));
            }
        }
    }
    Ok(())
}

fn to_input(
    state: &State,
    places: &HashMap<String, u8>,
    item: &PiggyBank,
) -> Result<NewPiggyBank, String> {
    let currency = item.currency_code.clone();
    let target_amount = match (&item.target_amount, &currency) {
        (Some(text), Some(code)) => {
            let places = *places
                .get(code)
                .ok_or_else(|| format!("unknown currency {code}"))?;
            let normalized = normalize(text, places)?;
            // Firefly sends "0" for "no target".
            let zero = normalized
                .trim_start_matches('-')
                .bytes()
                .all(|b| b == b'0' || b == b'.');
            (!zero).then_some(normalized)
        }
        (Some(_), None) => return Err("target amount without currency".to_string()),
        (None, _) => None,
    };
    let mut account_ids = Vec::new();
    for account in &item.accounts {
        let local = state
            .refs
            .resolve(ACCOUNT_REF, &account.account_id)
            .and_then(|id| id.parse::<AccountId>().ok())
            .ok_or_else(|| format!("account {} was not imported", account.account_id))?;
        account_ids.push(local);
    }
    Ok(NewPiggyBank {
        name: item.name.trim().to_string(),
        currency,
        target_amount,
        start_date: item.start_date.as_deref().and_then(date_part),
        target_date: item.target_date.as_deref().and_then(date_part),
        active: Some(item.active.unwrap_or(true)),
        sort_order: Some(item.order.unwrap_or(0)),
        notes: item.notes.clone().filter(|n| !n.trim().is_empty()),
        account_ids,
    })
}

/// Whether the stored piggy bank already equals what the import would write.
fn same(view: &PiggyBankView, input: &NewPiggyBank) -> bool {
    let text = |d: Option<NaiveDate>| d.map(|d| d.to_string());
    let linked: BTreeSet<String> = view.accounts.iter().map(|a| a.account_id.clone()).collect();
    let wanted: BTreeSet<String> = input.account_ids.iter().map(ToString::to_string).collect();
    view.name == input.name
        && input.currency.as_deref().is_none_or(|c| c == view.currency)
        && view.target_amount == input.target_amount
        && view.start_date == text(input.start_date)
        && view.target_date == text(input.target_date)
        && Some(view.active) == input.active
        && Some(view.sort_order) == input.sort_order
        && view.notes == input.notes
        && linked == wanted
}

fn as_update(input: NewPiggyBank) -> PiggyBankUpdate {
    PiggyBankUpdate {
        name: Some(input.name),
        target_amount: Some(input.target_amount),
        start_date: Some(input.start_date),
        target_date: Some(input.target_date),
        active: input.active,
        sort_order: input.sort_order,
        notes: Some(input.notes),
        account_ids: Some(input.account_ids),
    }
}

/// Events of every imported piggy bank. Events never change in Firefly, so
/// one that was imported before is left alone.
pub async fn import_piggy_bank_events(
    cx: &ImportCx,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    let piggy_banks = {
        let guard = cx.state.lock().await;
        guard.refs.entries(REF)
    };
    for (firefly_id, local_id) in piggy_banks {
        let path = format!("/v1/piggy-banks/{firefly_id}/events");
        let events = cx.firefly.list::<PiggyBankEvent>(&path, &[]).await?;
        let Ok(piggy) = local_id.parse::<PiggyBankId>() else {
            continue;
        };
        let mut guard = cx.state.lock().await;
        let state = &mut *guard;
        let Ok(view) = cx
            .app
            .piggy_banks
            .get_in(&mut state.tx, &cx.ctx, piggy)
            .await
        else {
            report.note("piggy bank events skipped: piggy bank no longer exists");
            continue;
        };
        let places = places(state).await?;
        for event in events {
            if state.refs.resolve(EVENT_REF, &event.id).is_some() {
                report.unchanged(EVENTS);
                continue;
            }
            match import_event(cx, state, &places, &view, piggy, &event.attributes, report).await? {
                Ok(local) => {
                    state
                        .refs
                        .upsert(&mut state.tx, EVENT_REF, &event.id, &local, &cx.now())
                        .await?;
                    report.created(EVENTS);
                }
                Err(why) => report.skipped(EVENTS, format!("event {}: {why}", event.id)),
            }
        }
    }
    Ok(())
}

/// Writes one event; `Ok(Err(why))` skips it, `Err` stops the import.
async fn import_event(
    cx: &ImportCx,
    state: &mut State,
    places: &HashMap<String, u8>,
    view: &PiggyBankView,
    piggy: PiggyBankId,
    event: &PiggyBankEvent,
    report: &mut ImportReport,
) -> Result<Result<String, String>, ImportError> {
    let Some(places) = places.get(&view.currency).copied() else {
        return Ok(Err(format!("unknown currency {}", view.currency)));
    };
    let amount = match normalize(&event.amount, places) {
        Ok(amount) => amount,
        Err(why) => return Ok(Err(why)),
    };
    let Some(date) = date_part(&event.created_at) else {
        return Ok(Err(format!("bad date {:?}", event.created_at)));
    };
    let split_id = event
        .transaction_journal_id
        .as_deref()
        .and_then(|journal| state.refs.resolve(SPLIT_REF, journal))
        .and_then(|id| id.parse::<SplitId>().ok());
    let mut linked_side = None;
    if let Some(split) = split_id {
        linked_side = cx
            .app
            .piggy_banks
            .linked_side_of_split_in(&mut state.tx, &cx.ctx, piggy, split)
            .await?;
    }
    if event.transaction_journal_id.is_some() && linked_side.is_none() {
        report.note("piggy bank event: transaction not linked (not imported or not a transfer of a linked account)");
    }
    // Without a usable journal, Firefly does not say which account: use the
    // first linked one.
    let account_id = linked_side.or_else(|| {
        view.accounts
            .first()
            .and_then(|a| a.account_id.parse::<AccountId>().ok())
    });
    let input = NewPiggyBankEvent {
        account_id,
        amount,
        date,
        split_id: split_id.filter(|_| linked_side.is_some()),
    };
    match cx
        .app
        .piggy_banks
        .add_event_in(&mut state.tx, &cx.ctx, Origin::Import, piggy, input)
        .await
    {
        Ok(created) => Ok(Ok(created.id)),
        Err(error) => Ok(Err(super::skippable(error)?)),
    }
}
