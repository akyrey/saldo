//! `/v1/budgets` and `/v1/budgets/{id}/limits` → `budgets`, `budget_limits`
//! (ARCHITECTURE.md 9.1, row 4). Auto-budget fields map 1:1; `none` and
//! Firefly's `adjusted` type (which Saldo does not have) import without an
//! automatic limit.

use std::collections::HashMap;

use chrono::NaiveDate;
use saldo_app::budgets::{
    AutoBudgetInput, BudgetLimitUpdate, BudgetLimitView, BudgetUpdate, BudgetView, NewBudget,
    NewBudgetLimit,
};
use saldo_app::ctx::Origin;
use saldo_app::currencies;
use saldo_app::error::AppError;
use saldo_core::BudgetId;

use crate::amounts::normalize;
use crate::engine::{ImportCx, ImportError, State};
use crate::firefly::models::{Budget, BudgetLimit, Resource};
use crate::report::ImportReport;

const ENTITY: &str = "budgets";
const LIMITS: &str = "budget_limits";
pub const REF: &str = "budget";
const LIMIT_REF: &str = "budget_limit";

/// Decimal places per currency code.
type Places = HashMap<String, u8>;

pub async fn import_budgets(cx: &ImportCx, report: &mut ImportReport) -> Result<(), ImportError> {
    let budgets = cx.firefly.list::<Budget>("/v1/budgets", &[]).await?;
    // Read everything from Firefly before taking the database lock.
    let mut fetched = Vec::with_capacity(budgets.len());
    for budget in budgets {
        let path = format!("/v1/budgets/{}/limits", budget.id);
        let limits = cx.firefly.list::<BudgetLimit>(&path, &[]).await?;
        fetched.push((budget, limits));
    }
    let mut guard = cx.state.lock().await;
    let state = &mut *guard;
    let places: Places = currencies::all_in(&mut state.tx)
        .await?
        .into_iter()
        .map(|c| (c.code, c.decimal_places))
        .collect();
    for (budget, limits) in fetched {
        let Some(local) = import_budget(cx, state, report, &places, &budget).await? else {
            for limit in limits {
                report.skipped(
                    LIMITS,
                    format!("limit {}: its budget was skipped", limit.id),
                );
            }
            continue;
        };
        for limit in limits {
            import_limit(cx, state, report, &places, local, &limit).await?;
        }
    }
    Ok(())
}

/// The budget as Saldo input. Problems with the automatic limit are notes;
/// the budget is imported without it.
fn to_input(budget: &Budget, places: &Places, report: &mut ImportReport) -> NewBudget {
    let auto_budget = match budget.auto_budget_type.as_deref() {
        None | Some("none") => None,
        Some(kind @ ("reset" | "rollover")) => match auto_input(kind, budget, places) {
            Ok(auto) => Some(auto),
            Err(why) => {
                report.note(&format!("auto-budget not imported: {why}"));
                None
            }
        },
        Some(other) => {
            report.note(&format!(
                "auto-budget type {other} not supported, imported without it"
            ));
            None
        }
    };
    NewBudget {
        name: budget.name.trim().to_string(),
        active: budget.active.unwrap_or(true),
        sort_order: budget.order.unwrap_or(0),
        notes: clean(budget.notes.as_deref()),
        auto_budget,
    }
}

fn auto_input(kind: &str, budget: &Budget, places: &Places) -> Result<AutoBudgetInput, String> {
    let (Some(period), Some(amount), Some(code)) = (
        &budget.auto_budget_period,
        &budget.auto_budget_amount,
        &budget.currency_code,
    ) else {
        return Err("period, amount or currency missing".to_string());
    };
    let decimals = places
        .get(code)
        .copied()
        .ok_or_else(|| format!("unknown currency {code}"))?;
    Ok(AutoBudgetInput {
        auto_type: kind.to_string(),
        period: period.clone(),
        amount: normalize(amount, decimals)?,
        currency: Some(code.clone()),
    })
}

fn clean(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// True if the stored budget already equals the input.
fn same(input: &NewBudget, view: &BudgetView) -> bool {
    let auto_same = match (&input.auto_budget, &view.auto_budget) {
        (None, None) => true,
        (Some(a), Some(v)) => {
            a.auto_type == v.auto_type
                && a.period == v.period
                && a.amount == v.amount
                && a.currency.as_deref() == Some(v.currency.as_str())
        }
        _ => false,
    };
    input.name == view.name
        && input.active == view.active
        && input.sort_order == view.sort_order
        && input.notes == view.notes
        && auto_same
}

/// Creates or updates one budget; returns its local ID, or `None` if skipped.
async fn import_budget(
    cx: &ImportCx,
    state: &mut State,
    report: &mut ImportReport,
    places: &Places,
    item: &Resource<Budget>,
) -> Result<Option<BudgetId>, ImportError> {
    let input = to_input(&item.attributes, places, report);
    let service = &cx.app.budgets;
    let existing = state
        .refs
        .resolve(REF, &item.id)
        .and_then(|id| id.parse::<BudgetId>().ok());
    let current = match existing {
        Some(id) => service.get_in(&mut state.tx, &cx.ctx, id).await.ok(),
        None => None,
    };
    let result = match current {
        Some(view) if same(&input, &view) => {
            report.unchanged(ENTITY);
            return Ok(view.id.parse().ok());
        }
        Some(view) => {
            let update = BudgetUpdate {
                name: Some(input.name.clone()),
                active: Some(input.active),
                sort_order: Some(input.sort_order),
                notes: Some(input.notes.clone()),
                auto_budget: Some(input.auto_budget.clone()),
            };
            let id = parse_id(&view.id)?;
            service
                .update_in(&mut state.tx, &cx.ctx, Origin::Import, id, update)
                .await
                .map(|v| (v, false))
        }
        None => service
            .create_in(&mut state.tx, &cx.ctx, Origin::Import, input)
            .await
            .map(|v| (v, true)),
    };
    record(cx, state, report, &item.id, &item.attributes.name, result).await
}

fn parse_id(id: &str) -> Result<BudgetId, ImportError> {
    id.parse()
        .map_err(|_| ImportError::Invalid(format!("bad id {id}")))
}

/// Records the outcome of a budget write in the refs and the report.
async fn record(
    cx: &ImportCx,
    state: &mut State,
    report: &mut ImportReport,
    external: &str,
    name: &str,
    result: Result<(BudgetView, bool), AppError>,
) -> Result<Option<BudgetId>, ImportError> {
    match result {
        Ok((view, created)) => {
            state
                .refs
                .upsert(&mut state.tx, REF, external, &view.id, &cx.now())
                .await?;
            if created {
                report.created(ENTITY);
            } else {
                report.updated(ENTITY);
            }
            Ok(Some(parse_id(&view.id)?))
        }
        Err(error) => {
            report.skipped(ENTITY, format!("{name:?}: {}", super::skippable(error)?));
            Ok(None)
        }
    }
}

/// The limit as Saldo input; `Err` explains why it cannot be imported.
fn limit_input(limit: &BudgetLimit, places: &Places) -> Result<NewBudgetLimit, String> {
    let date = |text: &str| {
        NaiveDate::parse_from_str(text.get(..10).unwrap_or_default(), "%Y-%m-%d")
            .map_err(|_| format!("bad date {text:?}"))
    };
    let decimals = places
        .get(&limit.currency_code)
        .copied()
        .ok_or_else(|| format!("unknown currency {}", limit.currency_code))?;
    Ok(NewBudgetLimit {
        start_date: date(&limit.start)?,
        end_date: date(&limit.end)?,
        amount: normalize(&limit.amount, decimals)?,
        currency: Some(limit.currency_code.clone()),
        notes: clean(limit.notes.as_deref()),
    })
}

fn same_limit(input: &NewBudgetLimit, view: &BudgetLimitView) -> bool {
    input.start_date.to_string() == view.start_date
        && input.end_date.to_string() == view.end_date
        && input.amount == view.amount
        && input.currency.as_deref() == Some(view.currency.as_str())
        && input.notes == view.notes
}

async fn import_limit(
    cx: &ImportCx,
    state: &mut State,
    report: &mut ImportReport,
    places: &Places,
    budget: BudgetId,
    item: &Resource<BudgetLimit>,
) -> Result<(), ImportError> {
    let input = match limit_input(&item.attributes, places) {
        Ok(input) => input,
        Err(why) => {
            report.skipped(LIMITS, format!("limit {}: {why}", item.id));
            return Ok(());
        }
    };
    let service = &cx.app.budgets;
    let current = match state.refs.resolve(LIMIT_REF, &item.id).map(str::to_string) {
        Some(id) => service.get_limit_in(&mut state.tx, &cx.ctx, &id).await.ok(),
        None => None,
    };
    let result = match current {
        Some(view) if view.budget_id == budget.to_string() && same_limit(&input, &view) => {
            report.unchanged(LIMITS);
            return Ok(());
        }
        Some(view) if view.budget_id == budget.to_string() => {
            let update = BudgetLimitUpdate {
                start_date: Some(input.start_date),
                end_date: Some(input.end_date),
                amount: Some(input.amount),
                currency: input.currency,
                notes: Some(input.notes),
            };
            service
                .update_limit_in(&mut state.tx, &cx.ctx, Origin::Import, &view.id, update)
                .await
                .map(|v| (v, false))
        }
        // New, or moved to another budget in Firefly (Saldo cannot move one).
        _ => service
            .create_limit_in(&mut state.tx, &cx.ctx, Origin::Import, budget, input)
            .await
            .map(|v| (v, true)),
    };
    match result {
        Ok((view, created)) => {
            state
                .refs
                .upsert(&mut state.tx, LIMIT_REF, &item.id, &view.id, &cx.now())
                .await?;
            if created {
                report.created(LIMITS);
            } else {
                report.updated(LIMITS);
            }
        }
        Err(error) => report.skipped(
            LIMITS,
            format!("limit {}: {}", item.id, super::skippable(error)?),
        ),
    }
    Ok(())
}
