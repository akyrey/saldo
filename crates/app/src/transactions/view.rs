//! The JSON shape of a transaction group, used by the API and in events.

use std::collections::HashMap;

use saldo_core::{Currency, CurrencyCode, Money};
use serde::Serialize;
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::db::json_list;
use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct TransactionGroupView {
    pub id: String,
    pub title: Option<String>,
    pub created_by_user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub splits: Vec<SplitView>,
}

/// One split. Amounts are decimal strings in their currency.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct SplitView {
    pub id: String,
    pub position: i64,
    /// `withdrawal`, `deposit`, `transfer`, `opening_balance` or `reconciliation`.
    pub kind: String,
    pub date: String,
    pub description: String,
    pub source_account_id: String,
    pub destination_account_id: String,
    pub amount: String,
    pub currency: String,
    pub foreign_amount: Option<String>,
    pub foreign_currency: Option<String>,
    pub category_id: Option<String>,
    pub budget_id: Option<String>,
    pub bill_id: Option<String>,
    pub tag_ids: Vec<String>,
    pub reconciled: bool,
    pub notes: Option<String>,
    #[schema(value_type = Object)]
    pub meta: Value,
    pub created_at: String,
    pub updated_at: String,
}

/// Loads the groups with these IDs, in the same order. IDs that do not exist
/// in the household are skipped.
pub(crate) async fn load(
    conn: &mut SqliteConnection,
    household_id: &str,
    group_ids: &[String],
) -> Result<Vec<TransactionGroupView>, AppError> {
    let ids = json_list(group_ids)?;
    let groups = sqlx::query!(
        "SELECT id, title, created_by_user_id, created_at, updated_at FROM transaction_groups
         WHERE household_id = ? AND id IN (SELECT value FROM json_each(?))",
        household_id,
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut splits = load_splits(conn, household_id, &ids).await?;

    let mut by_id: HashMap<String, TransactionGroupView> = groups
        .into_iter()
        .map(|g| {
            let view = TransactionGroupView {
                splits: splits.remove(&g.id).unwrap_or_default(),
                id: g.id.clone(),
                title: g.title,
                created_by_user_id: g.created_by_user_id,
                created_at: g.created_at,
                updated_at: g.updated_at,
            };
            (g.id, view)
        })
        .collect();
    Ok(group_ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

/// Splits of the groups in `ids_json`, grouped by group ID, in position order.
async fn load_splits(
    conn: &mut SqliteConnection,
    household_id: &str,
    ids_json: &str,
) -> Result<HashMap<String, Vec<SplitView>>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT id, group_id, position, kind, date, description, source_account_id,
                  destination_account_id, amount_minor, currency_code, foreign_amount_minor,
                  foreign_currency_code, category_id, budget_id, bill_id,
                  reconciled AS "reconciled: bool", notes, meta, created_at, updated_at
           FROM splits WHERE household_id = ? AND group_id IN (SELECT value FROM json_each(?))
           ORDER BY group_id, position"#,
        household_id,
        ids_json,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut tags = load_tags(conn, household_id, ids_json).await?;
    let places = decimal_places(conn).await?;

    let mut grouped: HashMap<String, Vec<SplitView>> = HashMap::new();
    for row in rows {
        let foreign_amount = match (row.foreign_amount_minor, &row.foreign_currency_code) {
            (Some(minor), Some(code)) => Some(format_amount(minor, code, &places)?),
            _ => None,
        };
        let view = SplitView {
            amount: format_amount(row.amount_minor, &row.currency_code, &places)?,
            tag_ids: tags.remove(&row.id).unwrap_or_default(),
            meta: serde_json::from_str(&row.meta)
                .map_err(|e| AppError::Internal(format!("corrupt split meta: {e}")))?,
            id: row.id,
            position: row.position,
            kind: row.kind,
            date: row.date,
            description: row.description,
            source_account_id: row.source_account_id,
            destination_account_id: row.destination_account_id,
            currency: row.currency_code,
            foreign_amount,
            foreign_currency: row.foreign_currency_code,
            category_id: row.category_id,
            budget_id: row.budget_id,
            bill_id: row.bill_id,
            reconciled: row.reconciled,
            notes: row.notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        };
        grouped.entry(row.group_id).or_default().push(view);
    }
    Ok(grouped)
}

/// Tag IDs per split ID.
async fn load_tags(
    conn: &mut SqliteConnection,
    household_id: &str,
    ids_json: &str,
) -> Result<HashMap<String, Vec<String>>, AppError> {
    let rows = sqlx::query!(
        "SELECT st.split_id, st.tag_id FROM split_tags st JOIN splits s ON s.id = st.split_id
         WHERE s.household_id = ? AND s.group_id IN (SELECT value FROM json_each(?))
         ORDER BY st.tag_id",
        household_id,
        ids_json,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut tags: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        tags.entry(row.split_id).or_default().push(row.tag_id);
    }
    Ok(tags)
}

async fn decimal_places(conn: &mut SqliteConnection) -> Result<HashMap<String, i64>, AppError> {
    let rows = sqlx::query!("SELECT code, decimal_places FROM currencies")
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.code, r.decimal_places))
        .collect())
}

fn format_amount(
    minor: i64,
    code: &str,
    places: &HashMap<String, i64>,
) -> Result<String, AppError> {
    let corrupt = || AppError::Internal(format!("unknown stored currency {code:?}"));
    let currency_code = CurrencyCode::new(code).map_err(|_| corrupt())?;
    let decimal_places = places
        .get(code)
        .and_then(|p| u8::try_from(*p).ok())
        .ok_or_else(corrupt)?;
    let currency = Currency::new(currency_code, decimal_places).map_err(|_| corrupt())?;
    Ok(Money::new(minor, currency_code).to_decimal_string(&currency))
}
