//! `/subscription-candidates`: repeating payments offered as bills (Phase 9),
//! and `/subscription-dismissals` to bring a dismissed one back.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::bills::BillView;
use saldo_app::ctx::Origin;
use saldo_app::subscriptions::{
    AcceptSubscription, SubscriptionCandidateView, SubscriptionDismissalView,
};
use saldo_core::{AccountId, SplitId};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::scope;
use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_candidates))
        .routes(routes!(accept_candidate))
        .routes(routes!(dismiss_candidate))
        .routes(routes!(restore_candidate))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CandidateQuery {
    /// `household` (default), `mine` or `user:<id>`: filters by the paying account's owner.
    pub scope: Option<String>,
}

/// Repeating withdrawals of the last 18 months that no bill covers and nobody
/// dismissed, by next due date. Computed on each request; unpaginated.
#[utoipa::path(get, path = "/subscription-candidates", tag = "subscriptions", params(CandidateQuery),
    responses((status = 200, body = Data<Vec<SubscriptionCandidateView>>), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_candidates(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<CandidateQuery>,
) -> Result<Json<Data<Vec<SubscriptionCandidateView>>>, ApiError> {
    let scope = scope(query.scope.as_deref())?;
    let found = state.app.subscriptions.candidates(&auth.ctx, scope).await?;
    Ok(Json(Data::new(found)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AcceptCandidateRequest {
    /// The expense account paid.
    pub account_id: Uuid,
    pub currency: String,
    /// The bill's name.
    pub name: String,
    /// `weekly`, `monthly`, `quarterly`, `half_year` or `yearly`.
    pub frequency: String,
    pub amount_min: String,
    pub amount_max: String,
    /// The past payments the bill covers: withdrawals to the account in the
    /// currency, without a bill.
    pub split_ids: Vec<Uuid>,
}

/// Creates a bill first due on the first payment, links the payments (which
/// pays their periods) and skips the periods left open before the last one.
#[utoipa::path(post, path = "/subscription-candidates/accept", tag = "subscriptions", request_body = AcceptCandidateRequest,
    responses((status = 201, body = Data<BillView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn accept_candidate(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<AcceptCandidateRequest>,
) -> Result<(StatusCode, Json<Data<BillView>>), ApiError> {
    let input = AcceptSubscription {
        account_id: AccountId(body.account_id),
        currency: body.currency,
        name: body.name,
        frequency: body.frequency,
        amount_min: body.amount_min,
        amount_max: body.amount_max,
        split_ids: body.split_ids.into_iter().map(SplitId).collect(),
    };
    let bill = state
        .app
        .subscriptions
        .accept(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(bill))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DismissCandidateRequest {
    /// An expense account.
    pub account_id: Uuid,
    pub currency: String,
}

/// Stops suggesting payments to the account in the currency. Dismissing
/// twice changes nothing.
#[utoipa::path(post, path = "/subscription-candidates/dismiss", tag = "subscriptions", request_body = DismissCandidateRequest,
    responses((status = 200, body = Data<SubscriptionDismissalView>), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn dismiss_candidate(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<DismissCandidateRequest>,
) -> Result<Json<Data<SubscriptionDismissalView>>, ApiError> {
    let view = state
        .app
        .subscriptions
        .dismiss(
            &auth.ctx,
            Origin::Api,
            AccountId(body.account_id),
            &body.currency,
        )
        .await?;
    Ok(Json(Data::new(view)))
}

/// Suggests payments to the account in the currency again.
#[utoipa::path(delete, path = "/subscription-dismissals/{account_id}/{currency}", tag = "subscriptions",
    params(("account_id" = Uuid, Path), ("currency" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn restore_candidate(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath((account_id, currency)): ApiPath<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .subscriptions
        .restore(&auth.ctx, Origin::Api, AccountId(account_id), &currency)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
