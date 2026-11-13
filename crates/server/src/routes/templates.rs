//! `/transaction-templates`: CRUD, and `POST /transactions/{id}/template`
//! (save an existing group as a template). There is no instantiate endpoint:
//! the web editor fills itself from a template and posts `/transactions`.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::ctx::Origin;
use saldo_app::templates::{
    NewTemplate, TemplateId, TemplateSplitInput, TemplateUpdate, TransactionTemplateView,
};
use saldo_core::{AccountId, BillId, BudgetId, CategoryId, GroupId, TagId};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::{double_option, page};
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_templates, create_template))
        .routes(routes!(get_template, update_template, delete_template))
        .routes(routes!(template_from_transaction))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TransactionTemplateQuery {
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// One partial split; every field is optional.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct TransactionTemplateSplitRequest {
    pub description: Option<String>,
    pub source_id: Option<Uuid>,
    pub destination_id: Option<Uuid>,
    /// Positive decimal string, for example `"12.50"`.
    pub amount: Option<String>,
    /// Defaults to the currency of the asset or liability side, else the
    /// household's primary currency.
    pub currency: Option<String>,
    pub foreign_amount: Option<String>,
    pub foreign_currency: Option<String>,
    pub category_id: Option<Uuid>,
    /// Withdrawals only.
    pub budget_id: Option<Uuid>,
    pub bill_id: Option<Uuid>,
    #[serde(default)]
    pub tag_ids: Vec<Uuid>,
    pub notes: Option<String>,
}

impl From<TransactionTemplateSplitRequest> for TemplateSplitInput {
    fn from(split: TransactionTemplateSplitRequest) -> Self {
        TemplateSplitInput {
            description: split.description,
            source_id: split.source_id.map(AccountId),
            destination_id: split.destination_id.map(AccountId),
            amount: split.amount,
            currency: split.currency,
            foreign_amount: split.foreign_amount,
            foreign_currency: split.foreign_currency,
            category_id: split.category_id.map(CategoryId),
            budget_id: split.budget_id.map(BudgetId),
            bill_id: split.bill_id.map(BillId),
            tag_ids: split.tag_ids.into_iter().map(TagId).collect(),
            notes: split.notes,
        }
    }
}

fn splits_of(splits: Vec<TransactionTemplateSplitRequest>) -> Vec<TemplateSplitInput> {
    splits.into_iter().map(TemplateSplitInput::from).collect()
}

/// Templates by `sort_order`, then name.
#[utoipa::path(get, path = "/transaction-templates", tag = "templates", params(TransactionTemplateQuery),
    responses((status = 200, body = List<TransactionTemplateView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_templates(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<TransactionTemplateQuery>,
) -> Result<Json<List<TransactionTemplateView>>, ApiError> {
    let result = state
        .app
        .templates
        .list(&auth.ctx, page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTransactionTemplateRequest {
    /// Unique in the household.
    pub name: String,
    /// `withdrawal`, `deposit` or `transfer`.
    pub kind: String,
    pub title: Option<String>,
    /// Defaults to after the last template.
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub splits: Vec<TransactionTemplateSplitRequest>,
}

#[utoipa::path(post, path = "/transaction-templates", tag = "templates", request_body = CreateTransactionTemplateRequest,
    responses((status = 201, body = Data<TransactionTemplateView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_template(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateTransactionTemplateRequest>,
) -> Result<(StatusCode, Json<Data<TransactionTemplateView>>), ApiError> {
    let input = NewTemplate {
        name: body.name,
        kind: body.kind,
        title: body.title,
        sort_order: body.sort_order,
        splits: splits_of(body.splits),
    };
    let view = state
        .app
        .templates
        .create(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

/// A template; references deleted since it was saved are left out.
#[utoipa::path(get, path = "/transaction-templates/{id}", tag = "templates", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<TransactionTemplateView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_template(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<TransactionTemplateView>>, ApiError> {
    let view = state.app.templates.get(&auth.ctx, TemplateId(id)).await?;
    Ok(Json(Data::new(view)))
}

/// Fields left out keep their value; `splits` replaces all splits.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct TransactionTemplatePatch {
    pub name: Option<String>,
    pub kind: Option<String>,
    /// `null` clears the title.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub title: Option<Option<String>>,
    pub sort_order: Option<i64>,
    pub splits: Option<Vec<TransactionTemplateSplitRequest>>,
}

#[utoipa::path(patch, path = "/transaction-templates/{id}", tag = "templates", params(("id" = Uuid, Path)), request_body = TransactionTemplatePatch,
    responses((status = 200, body = Data<TransactionTemplateView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_template(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<TransactionTemplatePatch>,
) -> Result<Json<Data<TransactionTemplateView>>, ApiError> {
    let update = TemplateUpdate {
        name: body.name,
        kind: body.kind,
        title: body.title,
        sort_order: body.sort_order,
        splits: body.splits.map(splits_of),
    };
    let view = state
        .app
        .templates
        .update(&auth.ctx, Origin::Api, TemplateId(id), update)
        .await?;
    Ok(Json(Data::new(view)))
}

#[utoipa::path(delete, path = "/transaction-templates/{id}", tag = "templates", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_template(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .templates
        .delete(&auth.ctx, Origin::Api, TemplateId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveAsTemplateRequest {
    /// Unique in the household.
    pub name: String,
}

/// Saves the group as a template, without its dates and reconciled flags.
/// Opening balances and reconciliations cannot be templates (422).
#[utoipa::path(post, path = "/transactions/{id}/template", tag = "templates", params(("id" = Uuid, Path)), request_body = SaveAsTemplateRequest,
    responses((status = 201, body = Data<TransactionTemplateView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn template_from_transaction(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<SaveAsTemplateRequest>,
) -> Result<(StatusCode, Json<Data<TransactionTemplateView>>), ApiError> {
    let view = state
        .app
        .templates
        .from_transaction(&auth.ctx, Origin::Api, GroupId(id), body.name)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}
