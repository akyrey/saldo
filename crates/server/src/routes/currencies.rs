//! `GET /currencies`.

use axum::Json;
use axum::extract::State;
use saldo_app::currencies::CurrencyView;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_currencies))
}

/// Every known currency, by code.
#[utoipa::path(get, path = "/currencies", tag = "currencies",
    responses((status = 200, body = Data<Vec<CurrencyView>>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_currencies(
    State(state): State<AppState>,
    Auth(_): Auth,
) -> Result<Json<Data<Vec<CurrencyView>>>, ApiError> {
    Ok(Json(Data::new(state.app.currencies.list().await?)))
}
