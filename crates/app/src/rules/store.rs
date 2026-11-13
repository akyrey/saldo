//! SQL for rule groups, rules, triggers and actions.

use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use crate::error::AppError;

/// A trigger as stored and as the API shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TriggerView {
    /// Firefly's keyword, for example `description_contains`.
    pub kind: String,
    pub value: Option<String>,
    #[serde(default)]
    pub negate: bool,
}

/// An action as stored and as the API shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ActionView {
    /// Firefly's keyword, for example `set_category`.
    pub kind: String,
    pub value: Option<String>,
    #[serde(default)]
    pub stop_processing: bool,
    #[serde(default = "yes")]
    pub active: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct RuleGroupView {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub sort_order: i64,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct RuleView {
    pub id: String,
    pub rule_group_id: String,
    pub title: String,
    pub description: Option<String>,
    /// `create`, `update` or `manual`.
    pub trigger_on: String,
    /// All triggers must match (`true`) or any one.
    pub strict: bool,
    /// Skip the group's later rules after this one matched.
    pub stop_processing: bool,
    pub active: bool,
    pub sort_order: i64,
    pub triggers: Vec<TriggerView>,
    pub actions: Vec<ActionView>,
    pub created_at: String,
    pub updated_at: String,
}

/// A rule row without its triggers and actions.
pub(crate) struct RuleRow {
    pub id: String,
    pub rule_group_id: String,
    pub title: String,
    pub description: Option<String>,
    pub trigger_on: String,
    pub strict: bool,
    pub stop_processing: bool,
    pub active: bool,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) async fn group(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<RuleGroupView, AppError> {
    sqlx::query_as!(
        RuleGroupView,
        r#"SELECT id, title, description, sort_order, active AS "active: bool", created_at, updated_at
           FROM rule_groups WHERE household_id = ? AND id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)
}

pub(crate) async fn groups(
    conn: &mut SqliteConnection,
    household_id: &str,
) -> Result<Vec<RuleGroupView>, AppError> {
    let rows = sqlx::query_as!(
        RuleGroupView,
        r#"SELECT id, title, description, sort_order, active AS "active: bool", created_at, updated_at
           FROM rule_groups WHERE household_id = ? ORDER BY sort_order, title"#,
        household_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// The household's rules (optionally of one group), in evaluation order:
/// group order, then rule order.
pub(crate) async fn rules(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_id: Option<&str>,
) -> Result<Vec<RuleView>, AppError> {
    let rows = sqlx::query_as!(
        RuleRow,
        r#"SELECT r.id, r.rule_group_id, r.title, r.description, r.trigger_on, r.strict AS "strict: bool",
                  r.stop_processing AS "stop_processing: bool", r.active AS "active: bool", r.sort_order,
                  r.created_at, r.updated_at
           FROM rules r JOIN rule_groups g ON g.id = r.rule_group_id
           WHERE r.household_id = ?1 AND (?2 IS NULL OR r.rule_group_id = ?2)
           ORDER BY g.sort_order, g.title, r.sort_order, r.title"#,
        household_id,
        group_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        views.push(with_parts(conn, row).await?);
    }
    Ok(views)
}

pub(crate) async fn rule(
    conn: &mut SqliteConnection,
    household_id: &str,
    id: &str,
) -> Result<RuleView, AppError> {
    let row = sqlx::query_as!(
        RuleRow,
        r#"SELECT id, rule_group_id, title, description, trigger_on, strict AS "strict: bool",
                  stop_processing AS "stop_processing: bool", active AS "active: bool", sort_order, created_at, updated_at
           FROM rules WHERE household_id = ? AND id = ?"#,
        household_id,
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    with_parts(conn, row).await
}

async fn with_parts(conn: &mut SqliteConnection, row: RuleRow) -> Result<RuleView, AppError> {
    let triggers = sqlx::query_as!(
        TriggerView,
        r#"SELECT kind, value, negate AS "negate: bool" FROM rule_triggers WHERE rule_id = ? ORDER BY position"#,
        row.id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let actions = sqlx::query_as!(
        ActionView,
        r#"SELECT kind, value, stop_processing AS "stop_processing: bool", active AS "active: bool"
           FROM rule_actions WHERE rule_id = ? ORDER BY position"#,
        row.id,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(RuleView {
        id: row.id,
        rule_group_id: row.rule_group_id,
        title: row.title,
        description: row.description,
        trigger_on: row.trigger_on,
        strict: row.strict,
        stop_processing: row.stop_processing,
        active: row.active,
        sort_order: row.sort_order,
        triggers,
        actions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Replaces a rule's triggers and actions with `triggers` and `actions`.
pub(crate) async fn write_parts(
    conn: &mut SqliteConnection,
    ids: &mut (dyn FnMut() -> String + Send),
    rule_id: &str,
    triggers: &[TriggerView],
    actions: &[ActionView],
) -> Result<(), AppError> {
    sqlx::query!("DELETE FROM rule_triggers WHERE rule_id = ?", rule_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query!("DELETE FROM rule_actions WHERE rule_id = ?", rule_id)
        .execute(&mut *conn)
        .await?;
    for (position, t) in triggers.iter().enumerate() {
        let (id, position) = (ids(), i64::try_from(position).unwrap_or(i64::MAX));
        sqlx::query!(
            "INSERT INTO rule_triggers (id, rule_id, position, kind, value, negate) VALUES (?, ?, ?, ?, ?, ?)",
            id, rule_id, position, t.kind, t.value, t.negate,
        )
        .execute(&mut *conn)
        .await?;
    }
    for (position, a) in actions.iter().enumerate() {
        let (id, position) = (ids(), i64::try_from(position).unwrap_or(i64::MAX));
        sqlx::query!(
            "INSERT INTO rule_actions (id, rule_id, position, kind, value, stop_processing, active) VALUES (?, ?, ?, ?, ?, ?, ?)",
            id, rule_id, position, a.kind, a.value, a.stop_processing, a.active,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
