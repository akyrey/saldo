//! `/budgets` and `/budget-limits`: CRUD for budgets and their limits.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::NaiveDate;
use saldo_app::budgets::{
    AutoBudgetInput, BudgetFilter, BudgetLimitUpdate, BudgetLimitView, BudgetUpdate, BudgetView,
    NewBudget, NewBudgetLimit,
};
use saldo_app::ctx::Origin;
use saldo_core::BudgetId;
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
        .routes(routes!(list_budgets, create_budget))
        .routes(routes!(get_budget, update_budget, delete_budget))
        .routes(routes!(list_limits, create_limit))
        .routes(routes!(update_limit, delete_limit))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BudgetQuery {
    /// Only active (`true`) or only inactive (`false`) budgets.
    pub active: Option<bool>,
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// Budgets ordered by `sort_order`, then name.
#[utoipa::path(get, path = "/budgets", tag = "budgets", params(BudgetQuery),
    responses((status = 200, body = List<BudgetView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_budgets(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<BudgetQuery>,
) -> Result<Json<List<BudgetView>>, ApiError> {
    let filter = BudgetFilter {
        active: query.active,
    };
    let result = state
        .app
        .budgets
        .list(&auth.ctx, &filter, page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}

/// An automatic limit: `amount` per `period`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AutoBudgetRequest {
    /// `reset` (the same amount every period) or `rollover` (plus what was left).
    #[serde(rename = "type")]
    pub auto_type: String,
    /// `daily`, `weekly`, `monthly`, `quarterly`, `half_year` or `yearly`.
    pub period: String,
    /// Decimal string, greater than zero.
    pub amount: String,
    /// Defaults to the household's primary currency.
    pub currency: Option<String>,
}

impl From<AutoBudgetRequest> for AutoBudgetInput {
    fn from(body: AutoBudgetRequest) -> Self {
        AutoBudgetInput {
            auto_type: body.auto_type,
            period: body.period,
            amount: body.amount,
            currency: body.currency,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBudgetRequest {
    pub name: String,
    /// Default `true`.
    pub active: Option<bool>,
    /// Default 0.
    pub sort_order: Option<i64>,
    pub notes: Option<String>,
    pub auto_budget: Option<AutoBudgetRequest>,
}

/// Creates a budget; an automatic limit also creates the current period's limit.
#[utoipa::path(post, path = "/budgets", tag = "budgets", request_body = CreateBudgetRequest,
    responses((status = 201, body = Data<BudgetView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_budget(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateBudgetRequest>,
) -> Result<(StatusCode, Json<Data<BudgetView>>), ApiError> {
    let input = NewBudget {
        name: body.name,
        active: body.active.unwrap_or(true),
        sort_order: body.sort_order.unwrap_or(0),
        notes: body.notes,
        auto_budget: body.auto_budget.map(AutoBudgetInput::from),
    };
    let view = state
        .app
        .budgets
        .create(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[utoipa::path(get, path = "/budgets/{id}", tag = "budgets", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<BudgetView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_budget(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<BudgetView>>, ApiError> {
    let view = state.app.budgets.get(&auth.ctx, BudgetId(id)).await?;
    Ok(Json(Data::new(view)))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct BudgetPatch {
    pub name: Option<String>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    /// `null` clears the notes.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub notes: Option<Option<String>>,
    /// `null` removes the automatic limit (existing limits stay).
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<AutoBudgetRequest>)]
    pub auto_budget: Option<Option<AutoBudgetRequest>>,
}

#[utoipa::path(patch, path = "/budgets/{id}", tag = "budgets", params(("id" = Uuid, Path)), request_body = BudgetPatch,
    responses((status = 200, body = Data<BudgetView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_budget(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<BudgetPatch>,
) -> Result<Json<Data<BudgetView>>, ApiError> {
    let update = BudgetUpdate {
        name: body.name,
        active: body.active,
        sort_order: body.sort_order,
        notes: body.notes,
        auto_budget: body.auto_budget.map(|a| a.map(AutoBudgetInput::from)),
    };
    let view = state
        .app
        .budgets
        .update(&auth.ctx, Origin::Api, BudgetId(id), update)
        .await?;
    Ok(Json(Data::new(view)))
}

/// Deletes the budget and its limits; splits that used it keep no budget.
#[utoipa::path(delete, path = "/budgets/{id}", tag = "budgets", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_budget(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .budgets
        .delete(&auth.ctx, Origin::Api, BudgetId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LimitQuery {
    /// Only limits ending on or after this date, `YYYY-MM-DD`.
    pub from: Option<NaiveDate>,
    /// Only limits starting on or before this date, `YYYY-MM-DD`.
    pub to: Option<NaiveDate>,
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// The budget's limits, newest first, with what was spent against each.
#[utoipa::path(get, path = "/budgets/{id}/limits", tag = "budgets", params(("id" = Uuid, Path), LimitQuery),
    responses((status = 200, body = List<BudgetLimitView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_limits(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<LimitQuery>,
) -> Result<Json<List<BudgetLimitView>>, ApiError> {
    let result = state
        .app
        .budgets
        .list_limits(
            &auth.ctx,
            BudgetId(id),
            query.from,
            query.to,
            page(query.page, query.per_page)?,
        )
        .await?;
    Ok(Json(result.into()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateLimitRequest {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    /// Decimal string, greater than zero.
    pub amount: String,
    /// Defaults to the household's primary currency.
    pub currency: Option<String>,
    pub notes: Option<String>,
}

/// Adds a limit; limits of one budget in one currency may not overlap (409).
#[utoipa::path(post, path = "/budgets/{id}/limits", tag = "budgets", params(("id" = Uuid, Path)), request_body = CreateLimitRequest,
    responses((status = 201, body = Data<BudgetLimitView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_limit(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CreateLimitRequest>,
) -> Result<(StatusCode, Json<Data<BudgetLimitView>>), ApiError> {
    let input = NewBudgetLimit {
        start_date: body.start_date,
        end_date: body.end_date,
        amount: body.amount,
        currency: body.currency,
        notes: body.notes,
    };
    let view = state
        .app
        .budgets
        .create_limit(&auth.ctx, Origin::Api, BudgetId(id), input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct LimitPatch {
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub amount: Option<String>,
    /// A new currency needs a new `amount` too.
    pub currency: Option<String>,
    /// `null` clears the notes.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub notes: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/budget-limits/{id}", tag = "budgets", params(("id" = Uuid, Path)), request_body = LimitPatch,
    responses((status = 200, body = Data<BudgetLimitView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_limit(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<LimitPatch>,
) -> Result<Json<Data<BudgetLimitView>>, ApiError> {
    let update = BudgetLimitUpdate {
        start_date: body.start_date,
        end_date: body.end_date,
        amount: body.amount,
        currency: body.currency,
        notes: body.notes,
    };
    let view = state
        .app
        .budgets
        .update_limit(&auth.ctx, Origin::Api, &id.to_string(), update)
        .await?;
    Ok(Json(Data::new(view)))
}

#[utoipa::path(delete, path = "/budget-limits/{id}", tag = "budgets", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_limit(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .budgets
        .delete_limit(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
