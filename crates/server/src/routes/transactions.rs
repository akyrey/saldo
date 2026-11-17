//! `/transactions`: groups are always written whole (POST, PUT, DELETE).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::NaiveDate;
use saldo_app::ctx::Origin;
use saldo_app::transactions::{
    SplitInput, TransactionFilter, TransactionGroupView, TransactionInput,
};
use saldo_core::{AccountId, BillId, BudgetId, CategoryId, GroupId, SplitId, TagId};
use serde::Deserialize;
use serde_json::{Map, Value};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::{page, scope};
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_transactions, create_transaction))
        .routes(routes!(
            get_transaction,
            replace_transaction,
            delete_transaction
        ))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TransactionQuery {
    /// First date, `YYYY-MM-DD`.
    pub from: Option<NaiveDate>,
    /// Last date, `YYYY-MM-DD`.
    pub to: Option<NaiveDate>,
    pub account_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub tag_id: Option<Uuid>,
    pub budget_id: Option<Uuid>,
    pub bill_id: Option<Uuid>,
    /// `withdrawal`, `deposit`, `transfer`, `opening_balance` or `reconciliation`.
    pub kind: Option<String>,
    /// Text in a split description or the group title.
    pub q: Option<String>,
    /// Only groups with a split the bank confirmed (`true`) or not yet (`false`).
    pub reconciled: Option<bool>,
    /// `household` (default), `mine` or `user:<id>`.
    pub scope: Option<String>,
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// Groups with at least one matching split, newest first.
#[utoipa::path(get, path = "/transactions", tag = "transactions", params(TransactionQuery),
    responses((status = 200, body = List<TransactionGroupView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_transactions(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<TransactionQuery>,
) -> Result<Json<List<TransactionGroupView>>, ApiError> {
    let filter = TransactionFilter {
        from: query.from,
        to: query.to,
        account_id: query.account_id.map(AccountId),
        category_id: query.category_id.map(CategoryId),
        tag_id: query.tag_id.map(TagId),
        budget_id: query.budget_id.map(BudgetId),
        bill_id: query.bill_id.map(BillId),
        kind: query.kind,
        q: query.q,
        reconciled: query.reconciled,
        scope: scope(query.scope.as_deref())?,
    };
    let result = state
        .app
        .transactions
        .list(&auth.ctx, &filter, page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TransactionRequest {
    pub title: Option<String>,
    pub splits: Vec<SplitRequest>,
}

/// One split. Money moves from `source_id` to `destination_id`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SplitRequest {
    /// `PUT` only: the existing split this one continues, keeping its ID and
    /// attachments. Leave it out for a new split; the group's splits that
    /// no entry names are deleted with their attachments.
    pub id: Option<Uuid>,
    /// `YYYY-MM-DD`.
    pub date: NaiveDate,
    pub description: String,
    pub source_id: Uuid,
    pub destination_id: Uuid,
    /// Positive decimal string, for example `"12.50"`.
    pub amount: String,
    /// Defaults to the currency of the asset or liability side.
    pub currency: Option<String>,
    pub foreign_amount: Option<String>,
    pub foreign_currency: Option<String>,
    pub category_id: Option<Uuid>,
    pub budget_id: Option<Uuid>,
    pub bill_id: Option<Uuid>,
    #[serde(default)]
    pub tag_ids: Vec<Uuid>,
    /// Whether the bank confirmed the split. Left out: `false` for a new
    /// split, the stored value for a split named by `id` in a `PUT`.
    pub reconciled: Option<bool>,
    pub notes: Option<String>,
    /// Free-form extra fields.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub meta: Map<String, Value>,
}

impl From<TransactionRequest> for TransactionInput {
    fn from(body: TransactionRequest) -> Self {
        let splits = body
            .splits
            .into_iter()
            .map(|split| SplitInput {
                id: split.id.map(SplitId),
                date: split.date,
                description: split.description,
                source_id: AccountId(split.source_id),
                destination_id: AccountId(split.destination_id),
                amount: split.amount,
                currency: split.currency,
                foreign_amount: split.foreign_amount,
                foreign_currency: split.foreign_currency,
                category_id: split.category_id.map(CategoryId),
                budget_id: split.budget_id.map(BudgetId),
                bill_id: split.bill_id.map(BillId),
                tag_ids: split.tag_ids.into_iter().map(TagId).collect(),
                reconciled: split.reconciled.unwrap_or(false),
                notes: split.notes,
                meta: split.meta,
            })
            .collect();
        TransactionInput {
            title: body.title,
            splits,
        }
    }
}

/// Creates a group with all its splits. Ledger rule failures return 422 with
/// one detail per broken rule.
#[utoipa::path(post, path = "/transactions", tag = "transactions", request_body = TransactionRequest,
    responses((status = 201, body = Data<TransactionGroupView>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn create_transaction(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<TransactionRequest>,
) -> Result<(StatusCode, Json<Data<TransactionGroupView>>), ApiError> {
    let view = state
        .app
        .transactions
        .create(&auth.ctx, Origin::Api, body.into())
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[utoipa::path(get, path = "/transactions/{id}", tag = "transactions", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<TransactionGroupView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_transaction(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<TransactionGroupView>>, ApiError> {
    Ok(Json(Data::new(
        state.app.transactions.get(&auth.ctx, GroupId(id)).await?,
    )))
}

/// Replaces the title and every split of a group.
#[utoipa::path(put, path = "/transactions/{id}", tag = "transactions", params(("id" = Uuid, Path)), request_body = TransactionRequest,
    responses((status = 200, body = Data<TransactionGroupView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn replace_transaction(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<TransactionRequest>,
) -> Result<Json<Data<TransactionGroupView>>, ApiError> {
    let body = keep_reconciled(&state, &auth.ctx, GroupId(id), body).await?;
    Ok(Json(Data::new(
        state
            .app
            .transactions
            .replace(&auth.ctx, Origin::Api, GroupId(id), body.into())
            .await?,
    )))
}

/// Fills in `reconciled` for the named splits that leave it out with their
/// stored value, so a client that does not know the flag (the web editor)
/// does not clear it on every save.
async fn keep_reconciled(
    state: &AppState,
    ctx: &saldo_app::ctx::HouseholdCtx,
    id: GroupId,
    mut body: TransactionRequest,
) -> Result<TransactionRequest, ApiError> {
    if body
        .splits
        .iter()
        .all(|s| s.reconciled.is_some() || s.id.is_none())
    {
        return Ok(body);
    }
    let stored = state.app.transactions.get(ctx, id).await?;
    for split in body.splits.iter_mut().filter(|s| s.reconciled.is_none()) {
        let Some(split_id) = split.id else { continue };
        split.reconciled = stored
            .splits
            .iter()
            .find(|s| s.id == split_id.to_string())
            .map(|s| s.reconciled);
    }
    Ok(body)
}

#[utoipa::path(delete, path = "/transactions/{id}", tag = "transactions", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_transaction(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .transactions
        .delete(&auth.ctx, Origin::Api, GroupId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
