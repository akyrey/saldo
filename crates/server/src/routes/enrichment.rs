//! `/accounts/{id}/enrichment`: existing transactions that stored Amazon or
//! PayPal rows explain, and accepting their details (Phase 13, Track E).

use axum::Json;
use axum::extract::State;
use chrono::NaiveDate;
use saldo_app::ctx::Origin;
use saldo_app::statements::{EnrichmentAccept, EnrichmentCandidate};
use saldo_app::transactions::TransactionGroupView;
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
        .routes(routes!(list_enrichment_candidates))
        .routes(routes!(accept_enrichment_candidates))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RangeQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// The account's transactions in the range that a stored Amazon or PayPal
/// row explains, with what it proposes (computed on each request).
#[utoipa::path(get, path = "/accounts/{id}/enrichment", tag = "sources",
    params(("id" = Uuid, Path), RangeQuery),
    responses((status = 200, body = Data<Vec<EnrichmentCandidate>>), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_enrichment_candidates(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<RangeQuery>,
) -> Result<Json<Data<Vec<EnrichmentCandidate>>>, ApiError> {
    let found = state
        .app
        .statements
        .enrichment_candidates(&auth.ctx, &id.to_string(), query.from, query.to)
        .await?;
    Ok(Json(Data::new(found)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AcceptItem {
    pub split_id: Uuid,
    /// Book a one-split Amazon purchase as one split per item.
    #[serde(default)]
    pub split_by_item: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AcceptRequest {
    pub items: Vec<AcceptItem>,
}

/// Books the accepted details: the proposal's description, the old text in
/// the notes, and optionally one split per Amazon item. 409 when nothing
/// explains a transaction any more.
#[utoipa::path(post, path = "/accounts/{id}/enrichment/accept", tag = "sources",
    params(("id" = Uuid, Path)), request_body = AcceptRequest,
    responses((status = 200, body = Data<Vec<TransactionGroupView>>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn accept_enrichment_candidates(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<AcceptRequest>,
) -> Result<Json<Data<Vec<TransactionGroupView>>>, ApiError> {
    let items = body
        .items
        .into_iter()
        .map(|item| EnrichmentAccept {
            split_id: item.split_id.to_string(),
            split_by_item: item.split_by_item,
        })
        .collect();
    let changed = state
        .app
        .statements
        .accept_enrichment(&auth.ctx, Origin::Api, &id.to_string(), items)
        .await?;
    Ok(Json(Data::new(changed)))
}
