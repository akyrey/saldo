//! `/piggy-banks`: CRUD plus adding and removing money (events).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::NaiveDate;
use saldo_app::ctx::Origin;
use saldo_app::piggy_banks::{
    NewPiggyBank, NewPiggyBankEvent, PiggyBankEventView, PiggyBankUpdate, PiggyBankView,
};
use saldo_core::{AccountId, PiggyBankId, SplitId};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::categories::PageQuery;
use super::{double_option, page};
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_piggy_banks, create_piggy_bank))
        .routes(routes!(
            get_piggy_bank,
            update_piggy_bank,
            delete_piggy_bank
        ))
        .routes(routes!(list_piggy_bank_events, create_piggy_bank_event))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PiggyBankQuery {
    /// Only active (`true`) or inactive (`false`) piggy banks.
    pub active: Option<bool>,
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// Piggy banks ordered by sort order and name.
#[utoipa::path(get, path = "/piggy-banks", tag = "piggy-banks", params(PiggyBankQuery),
    responses((status = 200, body = List<PiggyBankView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_piggy_banks(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<PiggyBankQuery>,
) -> Result<Json<List<PiggyBankView>>, ApiError> {
    let result = state
        .app
        .piggy_banks
        .list(&auth.ctx, query.active, page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePiggyBankRequest {
    pub name: String,
    /// Defaults to the household's primary currency.
    pub currency: Option<String>,
    /// Positive decimal string; omit for no target.
    pub target_amount: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub target_date: Option<NaiveDate>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    pub notes: Option<String>,
    /// Asset accounts in the piggy bank's currency; at least one.
    pub account_ids: Vec<Uuid>,
}

#[utoipa::path(post, path = "/piggy-banks", tag = "piggy-banks", request_body = CreatePiggyBankRequest,
    responses((status = 201, body = Data<PiggyBankView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_piggy_bank(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreatePiggyBankRequest>,
) -> Result<(StatusCode, Json<Data<PiggyBankView>>), ApiError> {
    let input = NewPiggyBank {
        name: body.name,
        currency: body.currency,
        target_amount: body.target_amount,
        start_date: body.start_date,
        target_date: body.target_date,
        active: body.active,
        sort_order: body.sort_order,
        notes: body.notes,
        account_ids: body.account_ids.into_iter().map(AccountId).collect(),
    };
    let view = state
        .app
        .piggy_banks
        .create(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[utoipa::path(get, path = "/piggy-banks/{id}", tag = "piggy-banks", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<PiggyBankView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_piggy_bank(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<PiggyBankView>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .piggy_banks
            .get(&auth.ctx, PiggyBankId(id))
            .await?,
    )))
}

/// Fields left out keep their value; `null` clears a nullable one.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PiggyBankPatch {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub target_amount: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<NaiveDate>)]
    pub start_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<NaiveDate>)]
    pub target_date: Option<Option<NaiveDate>>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub notes: Option<Option<String>>,
    /// Replaces the linked accounts; one still holding money cannot go (409).
    pub account_ids: Option<Vec<Uuid>>,
}

#[utoipa::path(patch, path = "/piggy-banks/{id}", tag = "piggy-banks", params(("id" = Uuid, Path)), request_body = PiggyBankPatch,
    responses((status = 200, body = Data<PiggyBankView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_piggy_bank(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<PiggyBankPatch>,
) -> Result<Json<Data<PiggyBankView>>, ApiError> {
    let update = PiggyBankUpdate {
        name: body.name,
        target_amount: body.target_amount,
        start_date: body.start_date,
        target_date: body.target_date,
        active: body.active,
        sort_order: body.sort_order,
        notes: body.notes,
        account_ids: body
            .account_ids
            .map(|ids| ids.into_iter().map(AccountId).collect()),
    };
    Ok(Json(Data::new(
        state
            .app
            .piggy_banks
            .update(&auth.ctx, Origin::Api, PiggyBankId(id), update)
            .await?,
    )))
}

/// Deletes the piggy bank and its events; the money stays in the accounts.
#[utoipa::path(delete, path = "/piggy-banks/{id}", tag = "piggy-banks", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_piggy_bank(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .piggy_banks
        .delete(&auth.ctx, Origin::Api, PiggyBankId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The piggy bank's events, newest first.
#[utoipa::path(get, path = "/piggy-banks/{id}/events", tag = "piggy-banks", params(("id" = Uuid, Path), PageQuery),
    responses((status = 200, body = List<PiggyBankEventView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_piggy_bank_events(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<PageQuery>,
) -> Result<Json<List<PiggyBankEventView>>, ApiError> {
    let result = state
        .app
        .piggy_banks
        .list_events(
            &auth.ctx,
            PiggyBankId(id),
            page(query.page, query.per_page)?,
        )
        .await?;
    Ok(Json(result.into()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PiggyBankEventRequest {
    /// Required when more than one account is linked.
    pub account_id: Option<Uuid>,
    /// Signed decimal string: `"25.00"` adds, `"-10.00"` removes.
    pub amount: String,
    pub date: NaiveDate,
    /// A transfer split into or out of the account that moved the money.
    pub split_id: Option<Uuid>,
}

/// Adds or removes money. Refused (422) if the account would hold less than
/// zero or more than its balance, or the total would pass the target.
#[utoipa::path(post, path = "/piggy-banks/{id}/events", tag = "piggy-banks", params(("id" = Uuid, Path)), request_body = PiggyBankEventRequest,
    responses((status = 201, body = Data<PiggyBankEventView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_piggy_bank_event(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<PiggyBankEventRequest>,
) -> Result<(StatusCode, Json<Data<PiggyBankEventView>>), ApiError> {
    let input = NewPiggyBankEvent {
        account_id: body.account_id.map(AccountId),
        amount: body.amount,
        date: body.date,
        split_id: body.split_id.map(SplitId),
    };
    let view = state
        .app
        .piggy_banks
        .add_event(&auth.ctx, Origin::Api, PiggyBankId(id), input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}
