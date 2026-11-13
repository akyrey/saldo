//! The JSON shape of a template, and the references it may point to.

use std::collections::{HashMap, HashSet};

use saldo_core::Account;
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use crate::accounts::{core_account, repo as account_repo};
use crate::ctx::HouseholdCtx;
use crate::db::json_list;
use crate::error::AppError;
use crate::{bills, budgets, categories, tags};

/// A saved, partial transaction. The editor fills itself from it; using it
/// defaults the date to today.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct TemplateView {
    pub id: String,
    pub name: String,
    /// `withdrawal`, `deposit` or `transfer`: which accounts the editor
    /// offers when the template has none.
    pub kind: String,
    pub title: Option<String>,
    pub splits: Vec<TemplateSplitView>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// One partial split; every field may be missing. Amounts are decimal
/// strings. This is also the JSON stored in `transaction_templates.splits`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TemplateSplitView {
    pub description: Option<String>,
    pub source_id: Option<String>,
    pub destination_id: Option<String>,
    pub amount: Option<String>,
    pub currency: Option<String>,
    pub foreign_amount: Option<String>,
    pub foreign_currency: Option<String>,
    pub category_id: Option<String>,
    pub budget_id: Option<String>,
    pub bill_id: Option<String>,
    #[serde(default)]
    pub tag_ids: Vec<String>,
    pub notes: Option<String>,
}

/// The accounts, categories, budgets, bills and tags of the household that
/// some splits refer to. Anything missing is unknown (or deleted since).
#[derive(Debug, Default)]
pub(super) struct Known {
    pub accounts: HashMap<String, Account>,
    categories: HashSet<String>,
    budgets: HashSet<String>,
    bills: HashSet<String>,
    tags: HashSet<String>,
}

impl Known {
    /// Loads, filtered by household, everything `splits` refer to.
    pub async fn load<'a>(
        conn: &mut SqliteConnection,
        ctx: &HouseholdCtx,
        splits: impl Iterator<Item = &'a TemplateSplitView> + Clone,
    ) -> Result<Self, AppError> {
        // `'a` says the splits are borrowed from the caller for as long as
        // the iterator lives (like a Go slice pointing into the caller's array).
        let ids = |pick: fn(&TemplateSplitView) -> Vec<&String>| -> Result<String, AppError> {
            let list: Vec<String> = splits.clone().flat_map(pick).cloned().collect();
            json_list(&list)
        };
        let household_id = ctx.household_id.to_string();
        let account_ids = ids(|s| s.source_id.iter().chain(&s.destination_id).collect())?;
        let mut accounts = HashMap::new();
        for row in account_repo::get_many(conn, &household_id, &account_ids).await? {
            accounts.insert(row.id.clone(), core_account(&row)?);
        }
        let set = |found: Vec<String>| found.into_iter().collect::<HashSet<_>>();
        Ok(Self {
            accounts,
            categories: set(categories::existing(
                conn,
                ctx,
                &ids(|s| s.category_id.iter().collect())?,
            )
            .await?),
            budgets: set(
                budgets::existing(conn, ctx, &ids(|s| s.budget_id.iter().collect())?).await?,
            ),
            bills: set(bills::existing(conn, ctx, &ids(|s| s.bill_id.iter().collect())?).await?),
            tags: set(tags::existing(conn, ctx, &ids(|s| s.tag_ids.iter().collect())?).await?),
        })
    }

    /// The split without references to things that no longer exist.
    pub fn prune(&self, split: TemplateSplitView) -> TemplateSplitView {
        let keep = |id: Option<String>, set: &HashSet<String>| id.filter(|i| set.contains(i));
        let account = |id: Option<String>| id.filter(|i| self.accounts.contains_key(i));
        TemplateSplitView {
            source_id: account(split.source_id),
            destination_id: account(split.destination_id),
            category_id: keep(split.category_id, &self.categories),
            budget_id: keep(split.budget_id, &self.budgets),
            bill_id: keep(split.bill_id, &self.bills),
            tag_ids: split
                .tag_ids
                .into_iter()
                .filter(|t| self.tags.contains(t))
                .collect(),
            ..split
        }
    }

    /// The first reference of `split` that is unknown, as `(field, message)`.
    pub fn first_unknown(&self, split: &TemplateSplitView) -> Option<(&'static str, &'static str)> {
        let missing = |id: &Option<String>, set: &HashSet<String>| {
            id.as_ref().is_some_and(|i| !set.contains(i))
        };
        let missing_account =
            |id: &Option<String>| id.as_ref().is_some_and(|i| !self.accounts.contains_key(i));
        if missing_account(&split.source_id) {
            return Some(("source_id", "is not an account of this household"));
        }
        if missing_account(&split.destination_id) {
            return Some(("destination_id", "is not an account of this household"));
        }
        if missing(&split.category_id, &self.categories) {
            return Some(("category_id", "is not a category of this household"));
        }
        if missing(&split.budget_id, &self.budgets) {
            return Some(("budget_id", "is not a budget of this household"));
        }
        if missing(&split.bill_id, &self.bills) {
            return Some(("bill_id", "is not a bill of this household"));
        }
        if split.tag_ids.iter().any(|t| !self.tags.contains(t)) {
            return Some(("tag_ids", "contains a tag that is not of this household"));
        }
        None
    }
}

/// A stored row, before its splits are parsed and pruned.
pub(super) struct TemplateRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub title: Option<String>,
    pub splits: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Turns rows into views, dropping references deleted since they were saved.
pub(super) async fn views_of(
    conn: &mut SqliteConnection,
    ctx: &HouseholdCtx,
    rows: Vec<TemplateRow>,
) -> Result<Vec<TemplateView>, AppError> {
    let parsed = rows
        .into_iter()
        .map(|row| {
            let splits: Vec<TemplateSplitView> = serde_json::from_str(&row.splits)
                .map_err(|e| AppError::Internal(format!("corrupt template {}: {e}", row.id)))?;
            Ok((row, splits))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let known = Known::load(conn, ctx, parsed.iter().flat_map(|(_, s)| s)).await?;
    Ok(parsed
        .into_iter()
        .map(|(row, splits)| TemplateView {
            id: row.id,
            name: row.name,
            kind: row.kind,
            title: row.title,
            splits: splits.into_iter().map(|s| known.prune(s)).collect(),
            sort_order: row.sort_order,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
        .collect())
}
