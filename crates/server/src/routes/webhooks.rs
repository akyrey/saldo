//! `/webhooks` and `/webhook-deliveries`: owner-only endpoint management.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::ctx::Origin;
use saldo_app::webhooks::{
    DeliveryView, NewWebhook, WebhookUpdate, WebhookView, WebhookWithSecret,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::categories::PageQuery;
use super::page;
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_webhooks, create_webhook))
        .routes(routes!(get_webhook, update_webhook, delete_webhook))
        .routes(routes!(test_webhook))
        .routes(routes!(rotate_webhook_secret))
        .routes(routes!(list_webhook_deliveries))
        .routes(routes!(redeliver))
}

/// Every endpoint of the household, by name. Owner only.
#[utoipa::path(get, path = "/webhooks", tag = "webhooks",
    responses((status = 200, body = Data<Vec<WebhookView>>), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_webhooks(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<Vec<WebhookView>>>, ApiError> {
    Ok(Json(Data::new(state.app.webhooks.list(&auth.ctx).await?)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWebhookRequest {
    pub name: String,
    /// `http` or `https`; private addresses are allowed.
    pub url: String,
    /// Exact types, `<entity>.*` prefixes or `*`. Default `["*"]`.
    pub event_types: Option<Vec<String>>,
    /// Also send events written by the importer. Default false.
    #[serde(default)]
    pub include_import_events: bool,
}

/// Creates an endpoint. The response carries the signing secret, once.
#[utoipa::path(post, path = "/webhooks", tag = "webhooks", request_body = CreateWebhookRequest,
    responses((status = 201, body = Data<WebhookWithSecret>), (status = 403, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_webhook(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<Data<WebhookWithSecret>>), ApiError> {
    let input = NewWebhook {
        name: body.name,
        url: body.url,
        event_types: body.event_types,
        include_import_events: body.include_import_events,
    };
    let created = state
        .app
        .webhooks
        .create(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(created))))
}

#[utoipa::path(get, path = "/webhooks/{id}", tag = "webhooks", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<WebhookView>), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn get_webhook(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<WebhookView>>, ApiError> {
    let view = state.app.webhooks.get(&auth.ctx, &id.to_string()).await?;
    Ok(Json(Data::new(view)))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct WebhookPatch {
    pub name: Option<String>,
    pub url: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub include_import_events: Option<bool>,
    /// `true` re-enables a deactivated endpoint and clears its failure count.
    pub active: Option<bool>,
}

#[utoipa::path(patch, path = "/webhooks/{id}", tag = "webhooks", params(("id" = Uuid, Path)), request_body = WebhookPatch,
    responses((status = 200, body = Data<WebhookView>), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_webhook(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<WebhookPatch>,
) -> Result<Json<Data<WebhookView>>, ApiError> {
    let update = WebhookUpdate {
        name: body.name,
        url: body.url,
        event_types: body.event_types,
        include_import_events: body.include_import_events,
        active: body.active,
    };
    let view = state
        .app
        .webhooks
        .update(&auth.ctx, Origin::Api, &id.to_string(), update)
        .await?;
    Ok(Json(Data::new(view)))
}

/// Deletes the endpoint and its delivery log.
#[utoipa::path(delete, path = "/webhooks/{id}", tag = "webhooks", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_webhook(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .webhooks
        .delete(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TestQueued {
    /// The `webhook.test` event that will be delivered to this endpoint only.
    pub event_id: String,
}

/// Appends a `webhook.test` event that only this endpoint receives.
#[utoipa::path(post, path = "/webhooks/{id}/test", tag = "webhooks", params(("id" = Uuid, Path)),
    responses((status = 202, body = Data<TestQueued>), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn test_webhook(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<(StatusCode, Json<Data<TestQueued>>), ApiError> {
    let event_id = state
        .app
        .webhooks
        .test(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(Data::new(TestQueued { event_id })),
    ))
}

/// Replaces the signing secret; the new one is in the response, once.
#[utoipa::path(post, path = "/webhooks/{id}/rotate-secret", tag = "webhooks", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<WebhookWithSecret>), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn rotate_webhook_secret(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<WebhookWithSecret>>, ApiError> {
    let rotated = state
        .app
        .webhooks
        .rotate_secret(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(Json(Data::new(rotated)))
}

/// The endpoint's deliveries, newest event first.
#[utoipa::path(get, path = "/webhooks/{id}/deliveries", tag = "webhooks", params(("id" = Uuid, Path), PageQuery),
    responses((status = 200, body = List<DeliveryView>), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_webhook_deliveries(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<PageQuery>,
) -> Result<Json<List<DeliveryView>>, ApiError> {
    let result = state
        .app
        .webhooks
        .deliveries(
            &auth.ctx,
            &id.to_string(),
            page(query.page, query.per_page)?,
        )
        .await?;
    Ok(Json(result.into()))
}

/// Queues a delivery again, now, with a fresh attempt budget.
#[utoipa::path(post, path = "/webhook-deliveries/{id}/redeliver", tag = "webhooks", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<DeliveryView>), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn redeliver(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<DeliveryView>>, ApiError> {
    let view = state
        .app
        .webhooks
        .redeliver(&auth.ctx, &id.to_string())
        .await?;
    Ok(Json(Data::new(view)))
}
