//! API routes under `/api/v1`. Each feature module returns its own router;
//! adding a feature means one new line here.

pub mod health;

use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;

pub fn api_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
}
