//! `/accounts/{id}/reconciliations` and `/reconciliations/{id}`: confirming
//! an account against a bank statement (Phase 9).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::NaiveDate;
use saldo_app::ctx::Origin;
use saldo_app::reconciliations::{NewReconciliation, ReconciliationDraftView, ReconciliationView};
use saldo_core::AccountId;
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::page;
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_reconciliations, create_reconciliation))
        .routes(routes!(reconciliation_draft))
        .routes(routes!(delete_reconciliation))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// The account's reconciliations, latest statement first.
#[utoipa::path(get, path = "/accounts/{id}/reconciliations", tag = "reconciliations",
    params(("id" = Uuid, Path), PageQuery),
    responses((status = 200, body = List<ReconciliationView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_reconciliations(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<PageQuery>,
) -> Result<Json<List<ReconciliationView>>, ApiError> {
    let result = state
        .app
        .reconciliations
        .list(&auth.ctx, AccountId(id), page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DraftQuery {
    /// The statement's date, `YYYY-MM-DD`.
    pub statement_date: NaiveDate,
}

/// The cleared balance (reconciled splits and opening balances) up to the
/// statement date, and the splits up to it the bank has not confirmed yet.
#[utoipa::path(get, path = "/accounts/{id}/reconciliations/draft", tag = "reconciliations",
    params(("id" = Uuid, Path), DraftQuery),
    responses((status = 200, body = Data<ReconciliationDraftView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn reconciliation_draft(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<DraftQuery>,
) -> Result<Json<Data<ReconciliationDraftView>>, ApiError> {
    let draft = state
        .app
        .reconciliations
        .draft(&auth.ctx, AccountId(id), query.statement_date)
        .await?;
    Ok(Json(Data::new(draft)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateReconciliationRequest {
    /// `YYYY-MM-DD`; not before the account's last reconciliation.
    pub statement_date: NaiveDate,
    /// The statement's closing balance in the account's currency (incoming
    /// minus outgoing: negative for money owed).
    pub statement_balance: String,
    /// Unreconciled splits of the account dated on or before the statement.
    pub split_ids: Vec<Uuid>,
    /// Book a difference as a reconciliation transaction; otherwise a
    /// difference is a 422 on `statement_balance`. Default `false`.
    pub book_difference: Option<bool>,
}

/// Marks the splits reconciled; with `book_difference`, books what the
/// statement has more (or less) as a reconciliation transaction.
#[utoipa::path(post, path = "/accounts/{id}/reconciliations", tag = "reconciliations",
    params(("id" = Uuid, Path)), request_body = CreateReconciliationRequest,
    responses((status = 201, body = Data<ReconciliationView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_reconciliation(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CreateReconciliationRequest>,
) -> Result<(StatusCode, Json<Data<ReconciliationView>>), ApiError> {
    let input = NewReconciliation {
        statement_date: body.statement_date,
        statement_balance: body.statement_balance,
        split_ids: body.split_ids.iter().map(Uuid::to_string).collect(),
        book_difference: body.book_difference.unwrap_or(false),
    };
    let view = state
        .app
        .reconciliations
        .create(&auth.ctx, Origin::Api, AccountId(id), input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

/// Undoes the account's latest reconciliation (409 for an older one).
#[utoipa::path(delete, path = "/reconciliations/{id}", tag = "reconciliations", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_reconciliation(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .reconciliations
        .delete(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
