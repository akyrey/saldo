//! The OpenAPI document, generated from the route handlers.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::routes::{self, health};
use crate::state::AppState;

/// Document-level information; paths come from the routers.
#[derive(OpenApi)]
#[openapi(
    info(title = "Saldo API", description = "Self-hosted personal finance. Amounts are decimal strings with a currency code."),
    modifiers(&SecurityAddon),
)]
pub struct ApiDoc;

/// Declares the two ways to authenticate: a bearer token or the session cookie.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
        components.add_security_scheme(
            "cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("saldo_session"))),
        );
    }
}

/// Every route with its documentation. Registering a handler here or in a
/// feature module also adds it to the document, so they cannot drift apart.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health::healthz))
        .nest("/api/v1", routes::api_router())
}

/// The document as pretty JSON (what `saldo openapi` prints).
pub fn document_json() -> Result<String, serde_json::Error> {
    router().into_openapi().to_pretty_json()
}
