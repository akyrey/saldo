//! Checks before a rule is saved: its keywords parse, and every ID it refers
//! to belongs to the household.

use saldo_core::{Action, Trigger};
use sqlx::SqliteConnection;

use super::store::{ActionView, TriggerView};
use crate::error::AppError;

pub const TRIGGER_ON: [&str; 3] = ["create", "update", "manual"];

/// Parses the triggers and actions of an active rule. An inactive rule may
/// keep keywords Saldo does not support (imported from Firefly as written),
/// but it cannot be activated until they are replaced.
pub(crate) async fn check(
    conn: &mut SqliteConnection,
    household_id: &str,
    active: bool,
    triggers: &[TriggerView],
    actions: &[ActionView],
) -> Result<(), AppError> {
    for (i, t) in triggers.iter().enumerate() {
        match Trigger::parse(&t.kind, t.value.as_deref()) {
            Ok(trigger) => check_trigger(conn, household_id, i, &trigger).await?,
            Err(error) if active => {
                return Err(AppError::invalid(
                    format!("triggers[{i}]"),
                    error.to_string(),
                ));
            }
            Err(_) => {}
        }
    }
    for (i, a) in actions.iter().enumerate() {
        match Action::parse(&a.kind, a.value.as_deref()) {
            Ok(action) => check_action(conn, household_id, i, &action).await?,
            Err(error) if active => {
                return Err(AppError::invalid(
                    format!("actions[{i}]"),
                    error.to_string(),
                ));
            }
            Err(_) => {}
        }
    }
    if active && (triggers.is_empty() || actions.is_empty()) {
        return Err(AppError::invalid(
            "triggers",
            "an active rule needs at least one trigger and one action",
        ));
    }
    Ok(())
}

async fn check_trigger(
    conn: &mut SqliteConnection,
    household_id: &str,
    i: usize,
    trigger: &Trigger,
) -> Result<(), AppError> {
    let field = format!("triggers[{i}]");
    match trigger {
        Trigger::CategoryIs(id) => {
            exists(conn, "categories", household_id, &id.to_string(), &field).await
        }
        Trigger::TagIs(id) => exists(conn, "tags", household_id, &id.to_string(), &field).await,
        Trigger::SourceAccountIs(id) | Trigger::DestinationAccountIs(id) => {
            exists(conn, "accounts", household_id, &id.to_string(), &field).await
        }
        _ => Ok(()),
    }
}

async fn check_action(
    conn: &mut SqliteConnection,
    household_id: &str,
    i: usize,
    action: &Action,
) -> Result<(), AppError> {
    let field = format!("actions[{i}]");
    match action {
        Action::SetCategory(id) => {
            exists(conn, "categories", household_id, &id.to_string(), &field).await
        }
        Action::SetBudget(id) => {
            exists(conn, "budgets", household_id, &id.to_string(), &field).await
        }
        Action::AddTag(id) | Action::RemoveTag(id) => {
            exists(conn, "tags", household_id, &id.to_string(), &field).await
        }
        Action::LinkToBill(id) => {
            exists(conn, "bills", household_id, &id.to_string(), &field).await
        }
        _ => Ok(()),
    }
}

/// Fails unless a row with `id` exists in `table` for the household. The
/// table name comes from the match arms above, never from input.
async fn exists(
    conn: &mut SqliteConnection,
    table: &'static str,
    household_id: &str,
    id: &str,
    field: &str,
) -> Result<(), AppError> {
    let found = match table {
        "categories" => {
            sqlx::query_scalar!(
                "SELECT EXISTS (SELECT 1 FROM categories WHERE household_id = ? AND id = ?)",
                household_id,
                id
            )
            .fetch_one(&mut *conn)
            .await?
        }
        "tags" => {
            sqlx::query_scalar!(
                "SELECT EXISTS (SELECT 1 FROM tags WHERE household_id = ? AND id = ?)",
                household_id,
                id
            )
            .fetch_one(&mut *conn)
            .await?
        }
        "accounts" => {
            sqlx::query_scalar!(
                "SELECT EXISTS (SELECT 1 FROM accounts WHERE household_id = ? AND id = ?)",
                household_id,
                id
            )
            .fetch_one(&mut *conn)
            .await?
        }
        "budgets" => {
            sqlx::query_scalar!(
                "SELECT EXISTS (SELECT 1 FROM budgets WHERE household_id = ? AND id = ?)",
                household_id,
                id
            )
            .fetch_one(&mut *conn)
            .await?
        }
        _ => {
            sqlx::query_scalar!(
                "SELECT EXISTS (SELECT 1 FROM bills WHERE household_id = ? AND id = ?)",
                household_id,
                id
            )
            .fetch_one(&mut *conn)
            .await?
        }
    };
    if found == 0 {
        return Err(AppError::invalid(
            field,
            format!("refers to an unknown {}", table.trim_end_matches('s')),
        ));
    }
    Ok(())
}
