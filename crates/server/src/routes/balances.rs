//! `GET /accounts/balances`: the balances of a page of accounts in one
//! request, so the web app's account list needs no request per account.

use axum::Json;
use axum::extract::State;
use chrono::NaiveDate;
use saldo_app::accounts::{AccountFilter, BalanceView};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::{page, scope};
use crate::auth::Auth;
use crate::envelope::List;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::ApiQuery;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(account_balances))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BalancesQuery {
    /// `asset`, `expense`, `revenue`, `liability` or `equity`.
    #[serde(rename = "type")]
    #[param(rename = "type")]
    pub account_type: Option<String>,
    pub active: Option<bool>,
    /// `household` (default), `mine` or `user:<id>`.
    pub scope: Option<String>,
    /// Part of the account name, ignoring case.
    pub q: Option<String>,
    /// `YYYY-MM-DD`; balances at the end of that day. Default: all time.
    pub date: Option<NaiveDate>,
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// Balances of the accounts `GET /accounts` returns for the same filters and
/// page, in the same order.
#[utoipa::path(get, path = "/accounts/balances", tag = "accounts", params(BalancesQuery),
    responses((status = 200, body = List<BalanceView>), (status = 401, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn account_balances(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<BalancesQuery>,
) -> Result<Json<List<BalanceView>>, ApiError> {
    let filter = AccountFilter {
        account_type: query.account_type,
        active: query.active,
        scope: scope(query.scope.as_deref())?,
        q: query.q,
    };
    let result = state
        .app
        .accounts
        .balances(
            &auth.ctx,
            &filter,
            query.date,
            page(query.page, query.per_page)?,
        )
        .await?;
    Ok(Json(result.into()))
}
