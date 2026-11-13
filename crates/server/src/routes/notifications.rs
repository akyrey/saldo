//! `/notification-channels` (the caller's own channels) and
//! `/push/vapid-key`. Responses never contain tokens or push keys.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::notifications::{ChannelKind, ChannelUpdate, ChannelView, Message, NewChannel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath};
use crate::notify::kind_name;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_channels, create_channel))
        .routes(routes!(update_channel, delete_channel))
        .routes(routes!(test_channel))
        .routes(routes!(vapid_key))
}

#[utoipa::path(get, path = "/notification-channels", tag = "notifications",
    responses((status = 200, body = Data<Vec<ChannelView>>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_channels(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<Vec<ChannelView>>>, ApiError> {
    Ok(Json(Data::new(
        state.app.notifications.list_channels(&auth.ctx).await?,
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateChannelRequest {
    /// `ntfy`, `gotify`, `email` or `webpush`.
    pub kind: String,
    pub name: String,
    /// ntfy: `{ server_url, topic, token? }`; gotify: `{ server_url, token }`;
    /// email: `{ to }`; webpush: the browser's `PushSubscription.toJSON()`.
    #[schema(value_type = std::collections::HashMap<String, serde_json::Value>)]
    pub config: Value,
    pub active: Option<bool>,
}

/// Email and Web Push need server settings; refuse those channels early.
fn check_available(state: &AppState, kind: &str) -> Result<(), AppError> {
    let kind = ChannelKind::parse(kind)?;
    if state.senders.for_kind(kind).is_none() {
        return Err(AppError::invalid(
            "kind",
            format!("{} is not configured on this server", kind_name(kind)),
        ));
    }
    Ok(())
}

#[utoipa::path(post, path = "/notification-channels", tag = "notifications", request_body = CreateChannelRequest,
    responses((status = 201, body = Data<ChannelView>), (status = 401, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_channel(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateChannelRequest>,
) -> Result<(StatusCode, Json<Data<ChannelView>>), ApiError> {
    check_available(&state, &body.kind)?;
    let input = NewChannel {
        kind: body.kind,
        name: body.name,
        config: body.config,
        active: body.active,
    };
    let view = state
        .app
        .notifications
        .create_channel(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ChannelPatch {
    pub name: Option<String>,
    /// Fields to change; others (such as a token you never see) are kept,
    /// and `null` removes an optional one.
    #[schema(value_type = Option<std::collections::HashMap<String, serde_json::Value>>)]
    pub config: Option<Value>,
    pub active: Option<bool>,
}

#[utoipa::path(patch, path = "/notification-channels/{id}", tag = "notifications", params(("id" = Uuid, Path)), request_body = ChannelPatch,
    responses((status = 200, body = Data<ChannelView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_channel(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<ChannelPatch>,
) -> Result<Json<Data<ChannelView>>, ApiError> {
    let update = ChannelUpdate {
        name: body.name,
        config: body.config,
        active: body.active,
    };
    Ok(Json(Data::new(
        state
            .app
            .notifications
            .update_channel(&auth.ctx, Origin::Api, id, update)
            .await?,
    )))
}

#[utoipa::path(delete, path = "/notification-channels/{id}", tag = "notifications", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_channel(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .notifications
        .delete_channel(&auth.ctx, Origin::Api, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TestResult {
    pub sent: bool,
    /// Why the send failed (never contains secrets).
    pub error: Option<String>,
}

/// Sends a test message now, also to a paused channel.
#[utoipa::path(post, path = "/notification-channels/{id}/test", tag = "notifications", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<TestResult>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn test_channel(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<TestResult>>, ApiError> {
    let channel = state.app.notifications.channel(&auth.ctx, id).await?;
    let result = match state.senders.send(&channel, &Message::test()).await {
        Ok(()) => TestResult {
            sent: true,
            error: None,
        },
        Err(error) => TestResult {
            sent: false,
            error: Some(error.to_string()),
        },
    };
    Ok(Json(Data::new(result)))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VapidKey {
    /// The base64url key for `pushManager.subscribe`; `null` when the server
    /// has no `SALDO_VAPID_PRIVATE_KEY` (Web Push is off).
    pub public_key: Option<String>,
}

#[utoipa::path(get, path = "/push/vapid-key", tag = "notifications",
    responses((status = 200, body = Data<VapidKey>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn vapid_key(State(state): State<AppState>, Auth(_auth): Auth) -> Json<Data<VapidKey>> {
    let public_key = state
        .config
        .notify
        .vapid
        .as_ref()
        .map(|v| v.public_key.clone());
    Json(Data::new(VapidKey { public_key }))
}
