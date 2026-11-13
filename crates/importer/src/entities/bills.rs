//! `/v1/subscriptions` → `bills` (ARCHITECTURE.md 9.1, row 5). Which periods
//! are paid follows from the transactions: an imported split linked to a
//! subscription pays the matching period when it is saved.

use std::collections::HashMap;

use chrono::NaiveDate;
use saldo_app::bills::{BillUpdate, BillView, NewBill};
use saldo_app::ctx::Origin;
use saldo_app::currencies;
use saldo_core::BillId;

use crate::amounts::normalize;
use crate::engine::{ImportCx, ImportError, State};
use crate::firefly::models::{Resource, Subscription};
use crate::report::ImportReport;

const ENTITY: &str = "bills";
/// Reference type of a bill; transactions resolve `subscription_id` by it.
pub const REF: &str = "bill";

pub async fn import_bills(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError> {
    let subscriptions = cx
        .firefly
        .list::<Subscription>("/v1/subscriptions", &[])
        .await?;
    let mut guard = cx.state.lock().await;
    let state = &mut *guard;
    let places: HashMap<String, u8> = currencies::all_in(&mut state.tx)
        .await?
        .into_iter()
        .map(|c| (c.code, c.decimal_places))
        .collect();
    for item in &subscriptions {
        let wanted = match desired(&item.attributes, &places) {
            Ok(wanted) => wanted,
            Err(why) => {
                report.skipped(ENTITY, format!("{:?}: {why}", item.attributes.name));
                continue;
            }
        };
        upsert(cx, state, report, item, wanted).await?;
    }
    Ok(())
}

/// Firefly's `half-year` is Saldo's `half_year`; the rest match.
fn frequency(firefly: &str) -> String {
    firefly.replace('-', "_")
}

/// The local date of a Firefly date-time (`2026-01-02T00:00:00+01:00`).
fn date_of(text: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(text.get(..10).unwrap_or_default(), "%Y-%m-%d")
        .map_err(|_| format!("bad date {text:?}"))
}

fn desired(s: &Subscription, places: &HashMap<String, u8>) -> Result<NewBill, String> {
    let currency_places = *places
        .get(&s.currency_code)
        .ok_or_else(|| format!("unknown currency {}", s.currency_code))?;
    Ok(NewBill {
        name: s.name.trim().to_string(),
        amount_min: normalize(&s.amount_min, currency_places)?,
        amount_max: normalize(&s.amount_max, currency_places)?,
        currency: Some(s.currency_code.clone()),
        first_due_date: date_of(&s.date)?,
        end_date: s.end_date.as_deref().map(date_of).transpose()?,
        repeat_freq: frequency(&s.repeat_freq),
        skip: Some(s.skip),
        remind_days_before: None,
        remind_every_days: None,
        active: Some(s.active),
        sort_order: s.order,
        notes: s
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string),
    })
}

/// True if the stored bill already looks like `wanted`. Reminder settings
/// are Saldo's own and are never overwritten by an import.
fn same(view: &BillView, wanted: &NewBill) -> bool {
    view.name == wanted.name
        && view.amount_min == wanted.amount_min
        && view.amount_max == wanted.amount_max
        && Some(&view.currency) == wanted.currency.as_ref()
        && view.first_due_date == wanted.first_due_date.to_string()
        && view.end_date == wanted.end_date.map(|d| d.to_string())
        && view.repeat_freq == wanted.repeat_freq
        && Some(view.skip) == wanted.skip.map(i64::from)
        && Some(view.active) == wanted.active
        && view.sort_order == wanted.sort_order.unwrap_or(0)
        && view.notes == wanted.notes
}

fn update_from(wanted: NewBill) -> BillUpdate {
    BillUpdate {
        name: Some(wanted.name),
        amount_min: Some(wanted.amount_min),
        amount_max: Some(wanted.amount_max),
        first_due_date: Some(wanted.first_due_date),
        end_date: Some(wanted.end_date),
        repeat_freq: Some(wanted.repeat_freq),
        skip: wanted.skip,
        remind_days_before: None,
        remind_every_days: None,
        active: wanted.active,
        sort_order: Some(wanted.sort_order.unwrap_or(0)),
        notes: Some(wanted.notes),
    }
}

async fn upsert(
    cx: &ImportCx,
    state: &mut State,
    report: &mut ImportReport,
    item: &Resource<Subscription>,
    wanted: NewBill,
) -> Result<(), ImportError> {
    let service = &cx.app.bills;
    let existing = state
        .refs
        .resolve(REF, &item.id)
        .and_then(|id| id.parse::<BillId>().ok());
    let current = match existing {
        Some(id) => service.get_in(&mut state.tx, &cx.ctx, id).await.ok(),
        None => None,
    };
    let name = wanted.name.clone();
    let result = match current {
        Some(view) if view.currency != wanted.currency.clone().unwrap_or_default() => {
            report.skipped(
                ENTITY,
                format!("{name:?}: currency changed in Firefly; Saldo keeps it fixed"),
            );
            return Ok(());
        }
        Some(view) if same(&view, &wanted) => {
            report.unchanged(ENTITY);
            return Ok(());
        }
        Some(view) => {
            let id: BillId = view
                .id
                .parse()
                .map_err(|_| ImportError::Invalid(format!("bad bill id {}", view.id)))?;
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
        Ok((local, created)) => {
            state
                .refs
                .upsert(&mut state.tx, REF, &item.id, &local, &cx.now())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_frequencies_and_dates() {
        assert_eq!(frequency("half-year"), "half_year");
        assert_eq!(frequency("monthly"), "monthly");
        assert_eq!(
            date_of("2026-01-02T00:00:00+01:00").unwrap().to_string(),
            "2026-01-02"
        );
        assert!(date_of("soon").is_err());
    }
}
