//! `GET /events`: the household's event log.

use axum::Json;
use axum::extract::State;
use saldo_app::events::{EventFilter, EventView};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::page;
use crate::auth::Auth;
use crate::envelope::List;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::ApiQuery;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_events))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventQuery {
    /// Only events after this sequence number.
    pub after_seq: Option<i64>,
    /// Only this type, for example `transaction.created`.
    #[serde(rename = "type")]
    #[param(rename = "type")]
    pub event_type: Option<String>,
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// Events in sequence order. Remember the last `seq` and pass it as `after_seq`.
#[utoipa::path(get, path = "/events", tag = "events", params(EventQuery),
    responses((status = 200, body = List<EventView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_events(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<EventQuery>,
) -> Result<Json<List<EventView>>, ApiError> {
    let filter = EventFilter {
        after_seq: query.after_seq,
        event_type: query.event_type,
    };
    let result = state
        .app
        .events
        .list(&auth.ctx, &filter, page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}
