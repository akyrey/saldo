//! `/accounts/{id}/credit-card...` and `/credit-cards`: a card's monthly
//! plan, its months and moving a transaction to the next month (Phase 12).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::credit_cards::{
    CardCycleDetailView, CardCycleView, CardSummaryView, CreditCardPlanView, PlanInput,
};
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_core::card_cycle::Cycle;
use saldo_core::{AccountId, SplitId};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_plan, put_plan, delete_plan))
        .routes(routes!(set_limit))
        .routes(routes!(list_cycles))
        .routes(routes!(get_cycle))
        .routes(routes!(move_split, unmove_split))
        .routes(routes!(list_summaries))
}

fn cycle(text: &str, field: &str) -> Result<Cycle, AppError> {
    text.parse()
        .map_err(|_| AppError::invalid(field, "must be a month, YYYY-MM"))
}

/// The card's plan: budgets, paying account and payment day.
#[utoipa::path(get, path = "/accounts/{id}/credit-card", tag = "credit-cards",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<CreditCardPlanView>), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn get_plan(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<CreditCardPlanView>>, ApiError> {
    let view = state
        .app
        .credit_cards
        .get_plan(&auth.ctx, AccountId(id))
        .await?;
    Ok(Json(Data::new(view)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlanRequest {
    /// The monthly budget in the card's currency. Required for a new plan;
    /// on an existing plan it sets the budget from the current month on.
    pub monthly_limit: Option<String>,
    /// An active asset account in the card's currency, not the card.
    pub settlement_account_id: Uuid,
    /// 1 to 31: the day of the following month the card is paid (the last
    /// day of a shorter month).
    pub settlement_day: i64,
}

/// Creates (201) or replaces (200) the plan of a credit card account.
#[utoipa::path(put, path = "/accounts/{id}/credit-card", tag = "credit-cards",
    params(("id" = Uuid, Path)), request_body = PlanRequest,
    responses((status = 200, body = Data<CreditCardPlanView>), (status = 201, body = Data<CreditCardPlanView>),
        (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn put_plan(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<PlanRequest>,
) -> Result<(StatusCode, Json<Data<CreditCardPlanView>>), ApiError> {
    let input = PlanInput {
        monthly_limit: body.monthly_limit,
        settlement_account_id: AccountId(body.settlement_account_id),
        settlement_day: body.settlement_day,
    };
    let (view, created) = state
        .app
        .credit_cards
        .put_plan(&auth.ctx, Origin::Api, AccountId(id), input)
        .await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(Data::new(view))))
}

/// Removes the plan. Settlement transfers the server still keeps in sync
/// and that are not due yet are deleted; booked ones stay.
#[utoipa::path(delete, path = "/accounts/{id}/credit-card", tag = "credit-cards",
    params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_plan(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .credit_cards
        .delete_plan(&auth.ctx, Origin::Api, AccountId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LimitRequest {
    /// The budget in the card's currency, more than zero.
    pub amount: String,
}

/// Sets the budget from a month on (the current one or later); earlier
/// months keep theirs.
#[utoipa::path(put, path = "/accounts/{id}/credit-card/limits/{cycle}", tag = "credit-cards",
    params(("id" = Uuid, Path), ("cycle" = String, Path, description = "YYYY-MM")),
    request_body = LimitRequest,
    responses((status = 200, body = Data<CreditCardPlanView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn set_limit(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath((id, month)): ApiPath<(Uuid, String)>,
    ApiJson(body): ApiJson<LimitRequest>,
) -> Result<Json<Data<CreditCardPlanView>>, ApiError> {
    let from = cycle(&month, "cycle")?;
    let view = state
        .app
        .credit_cards
        .set_limit(&auth.ctx, Origin::Api, AccountId(id), from, &body.amount)
        .await?;
    Ok(Json(Data::new(view)))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CyclesQuery {
    /// First month, `YYYY-MM`; default five months before `to`.
    pub from: Option<String>,
    /// Last month, `YYYY-MM`; default the current month. At most 36 months.
    pub to: Option<String>,
}

/// The card's months, newest first, from the plan's start at the earliest.
#[utoipa::path(get, path = "/accounts/{id}/credit-card/cycles", tag = "credit-cards",
    params(("id" = Uuid, Path), CyclesQuery),
    responses((status = 200, body = Data<Vec<CardCycleView>>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_cycles(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<CyclesQuery>,
) -> Result<Json<Data<Vec<CardCycleView>>>, ApiError> {
    let from = query
        .from
        .as_deref()
        .map(|t| cycle(t, "from"))
        .transpose()?;
    let to = query.to.as_deref().map(|t| cycle(t, "to")).transpose()?;
    let views = state
        .app
        .credit_cards
        .cycles(&auth.ctx, AccountId(id), from, to)
        .await?;
    Ok(Json(Data::new(views)))
}

/// One month with the transactions that count toward it.
#[utoipa::path(get, path = "/accounts/{id}/credit-card/cycles/{cycle}", tag = "credit-cards",
    params(("id" = Uuid, Path), ("cycle" = String, Path, description = "YYYY-MM")),
    responses((status = 200, body = Data<CardCycleDetailView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn get_cycle(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath((id, month)): ApiPath<(Uuid, String)>,
) -> Result<Json<Data<CardCycleDetailView>>, ApiError> {
    let wanted = cycle(&month, "cycle")?;
    let view = state
        .app
        .credit_cards
        .cycle(&auth.ctx, AccountId(id), wanted)
        .await?;
    Ok(Json(Data::new(view)))
}

/// Counts a purchase or refund of the card in the month after its date.
/// 409 when either month is closed (before the plan, or its payment booked).
#[utoipa::path(put, path = "/accounts/{id}/credit-card/moves/{split_id}", tag = "credit-cards",
    params(("id" = Uuid, Path), ("split_id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn move_split(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath((id, split)): ApiPath<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .credit_cards
        .move_split(&auth.ctx, Origin::Api, AccountId(id), SplitId(split))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Counts the transaction in its own month again.
#[utoipa::path(delete, path = "/accounts/{id}/credit-card/moves/{split_id}", tag = "credit-cards",
    params(("id" = Uuid, Path), ("split_id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn unmove_split(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath((id, split)): ApiPath<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .credit_cards
        .unmove_split(&auth.ctx, Origin::Api, AccountId(id), SplitId(split))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The current month of every card with a plan.
#[utoipa::path(get, path = "/credit-cards", tag = "credit-cards",
    responses((status = 200, body = Data<Vec<CardSummaryView>>), (status = 401, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_summaries(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<Vec<CardSummaryView>>>, ApiError> {
    let views = state.app.credit_cards.summaries(&auth.ctx).await?;
    Ok(Json(Data::new(views)))
}
