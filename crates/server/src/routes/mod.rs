//! API routes under `/api/v1`. Each feature module returns its own router;
//! adding a feature means one new line here.

pub mod auth;
pub mod health;
pub mod household;
pub mod setup;

use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;

pub fn api_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(setup::router())
        .merge(auth::router())
        .merge(household::router())
}
