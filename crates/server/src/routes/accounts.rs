//! `/accounts`: CRUD and balances.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::NaiveDate;
use saldo_app::accounts::{AccountFilter, AccountUpdate, AccountView, BalanceView, NewAccount};
use saldo_app::ctx::Origin;
use saldo_core::{AccountId, UserId};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::{double_option, page, scope};
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_accounts, create_account))
        .routes(routes!(get_account, update_account, delete_account))
        .routes(routes!(account_balance))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AccountQuery {
    /// `asset`, `expense`, `revenue`, `liability` or `equity`.
    #[serde(rename = "type")]
    #[param(rename = "type")]
    pub account_type: Option<String>,
    pub active: Option<bool>,
    /// `household` (default), `mine` or `user:<id>`.
    pub scope: Option<String>,
    /// Part of the account name, ignoring case.
    pub q: Option<String>,
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// Accounts ordered by type, sort order and name.
#[utoipa::path(get, path = "/accounts", tag = "accounts", params(AccountQuery),
    responses((status = 200, body = List<AccountView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_accounts(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<AccountQuery>,
) -> Result<Json<List<AccountView>>, ApiError> {
    let filter = AccountFilter {
        account_type: query.account_type,
        active: query.active,
        scope: scope(query.scope.as_deref())?,
        q: query.q,
    };
    let result = state
        .app
        .accounts
        .list(&auth.ctx, &filter, page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAccountRequest {
    pub name: String,
    /// `asset`, `expense`, `revenue` or `liability`.
    #[serde(rename = "type")]
    pub account_type: String,
    /// Omit for a joint account.
    pub owner_user_id: Option<Uuid>,
    /// Defaults to the household's primary currency.
    pub currency: Option<String>,
    pub asset_role: Option<String>,
    pub liability_type: Option<String>,
    pub liability_direction: Option<String>,
    pub interest_rate: Option<String>,
    pub interest_period: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub account_number: Option<String>,
    /// Decimal string.
    pub virtual_balance: Option<String>,
    pub include_net_worth: Option<bool>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    pub notes: Option<String>,
}

#[utoipa::path(post, path = "/accounts", tag = "accounts", request_body = CreateAccountRequest,
    responses((status = 201, body = Data<AccountView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_account(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateAccountRequest>,
) -> Result<(StatusCode, Json<Data<AccountView>>), ApiError> {
    let input = NewAccount {
        name: body.name,
        account_type: body.account_type,
        owner_user_id: body.owner_user_id.map(UserId),
        currency: body.currency,
        asset_role: body.asset_role,
        liability_type: body.liability_type,
        liability_direction: body.liability_direction,
        interest_rate: body.interest_rate,
        interest_period: body.interest_period,
        iban: body.iban,
        bic: body.bic,
        account_number: body.account_number,
        virtual_balance: body.virtual_balance,
        include_net_worth: body.include_net_worth,
        active: body.active,
        sort_order: body.sort_order,
        notes: body.notes,
    };
    let view = state
        .app
        .accounts
        .create(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[utoipa::path(get, path = "/accounts/{id}", tag = "accounts", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<AccountView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_account(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<AccountView>>, ApiError> {
    Ok(Json(Data::new(
        state.app.accounts.get(&auth.ctx, AccountId(id)).await?,
    )))
}

/// Fields to change. Missing fields stay; `null` clears a nullable field.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct AccountPatch {
    pub name: Option<String>,
    /// Accepted only if equal to the current type: types never change.
    #[serde(rename = "type")]
    pub account_type: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<Uuid>)]
    pub owner_user_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub asset_role: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub liability_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub liability_direction: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub interest_rate: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub interest_period: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub iban: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub bic: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub account_number: Option<Option<String>>,
    pub virtual_balance: Option<String>,
    pub include_net_worth: Option<bool>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub notes: Option<Option<String>>,
}

#[utoipa::path(patch, path = "/accounts/{id}", tag = "accounts", params(("id" = Uuid, Path)), request_body = AccountPatch,
    responses((status = 200, body = Data<AccountView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_account(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<AccountPatch>,
) -> Result<Json<Data<AccountView>>, ApiError> {
    let update = AccountUpdate {
        name: body.name,
        account_type: body.account_type,
        owner_user_id: body.owner_user_id.map(|owner| owner.map(UserId)),
        asset_role: body.asset_role,
        liability_type: body.liability_type,
        liability_direction: body.liability_direction,
        interest_rate: body.interest_rate,
        interest_period: body.interest_period,
        iban: body.iban,
        bic: body.bic,
        account_number: body.account_number,
        virtual_balance: body.virtual_balance,
        include_net_worth: body.include_net_worth,
        active: body.active,
        sort_order: body.sort_order,
        notes: body.notes,
    };
    Ok(Json(Data::new(
        state
            .app
            .accounts
            .update(&auth.ctx, Origin::Api, AccountId(id), update)
            .await?,
    )))
}

/// Deletes an account without transactions.
#[utoipa::path(delete, path = "/accounts/{id}", tag = "accounts", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_account(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .accounts
        .delete(&auth.ctx, Origin::Api, AccountId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BalanceQuery {
    /// `YYYY-MM-DD`; the balance at the end of that day. Default: all time.
    pub date: Option<NaiveDate>,
}

#[utoipa::path(get, path = "/accounts/{id}/balance", tag = "accounts", params(("id" = Uuid, Path), BalanceQuery),
    responses((status = 200, body = Data<BalanceView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn account_balance(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<BalanceQuery>,
) -> Result<Json<Data<BalanceView>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .accounts
            .balance(&auth.ctx, AccountId(id), query.date)
            .await?,
    )))
}
