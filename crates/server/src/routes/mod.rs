//! API routes under `/api/v1`. Each feature module returns its own router;
//! adding a feature means one new line here.

pub mod accounts;
pub mod attachments;
pub mod auth;
pub mod balances;
pub mod bills;
pub mod categories;
pub mod currencies;
pub mod events;
pub mod export;
pub mod health;
pub mod household;
pub mod setup;
pub mod tags;
pub mod transactions;

use saldo_app::error::AppError;
use saldo_app::page::PageRequest;
use saldo_app::scope::Scope;
use serde::{Deserialize, Deserializer};
use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;

pub fn api_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(setup::router())
        .merge(auth::router())
        .merge(household::router())
        .merge(currencies::router())
        .merge(balances::router())
        .merge(accounts::router())
        .merge(categories::router())
        .merge(tags::router())
        .merge(transactions::router())
        .merge(events::router())
        .merge(attachments::router())
        .merge(export::router())
        .merge(bills::router())
}

/// Checks `page` and `per_page` from a query string.
pub(crate) fn page(page: Option<u32>, per_page: Option<u32>) -> Result<PageRequest, AppError> {
    PageRequest::new(page, per_page)
}

/// Parses `scope`; missing means the whole household.
pub(crate) fn scope(text: Option<&str>) -> Result<Scope, AppError> {
    text.map_or(Ok(Scope::Household), Scope::parse)
}

/// For PATCH bodies: tells "field missing" (`None`) apart from "field set to
/// null" (`Some(None)`). Used with `#[serde(default, deserialize_with = ...)]`.
pub(crate) fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}
