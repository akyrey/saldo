//! `/reports/*`: net worth, cash flow, by category, by tag, budget vs actual.

use axum::Json;
use axum::extract::State;
use chrono::NaiveDate;
use saldo_app::reports::{
    BreakdownReport, BudgetReport, CashFlowReport, Interval, NetWorthReport, ReportQuery,
};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::scope;
use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::ApiQuery;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(net_worth))
        .routes(routes!(cash_flow))
        .routes(routes!(by_category))
        .routes(routes!(by_tag))
        .routes(routes!(budget_vs_actual))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReportParams {
    /// First date, `YYYY-MM-DD`.
    pub from: NaiveDate,
    /// Last date (inclusive), `YYYY-MM-DD`.
    pub to: NaiveDate,
    /// `day`, `week` (from Monday), `month` (default) or `year`; at most 1000 intervals.
    pub interval: Option<String>,
    /// `household` (default), `mine` or `user:<id>`: filters by account owner.
    pub scope: Option<String>,
}

impl ReportParams {
    fn query(&self) -> Result<ReportQuery, ApiError> {
        Ok(ReportQuery {
            from: self.from,
            to: self.to,
            interval: self
                .interval
                .as_deref()
                .map_or(Ok(Interval::default()), Interval::parse)?,
            scope: scope(self.scope.as_deref())?,
        })
    }
}

/// Asset and liability balances counted in net worth, at the end of each
/// interval, per account currency.
#[utoipa::path(get, path = "/reports/net-worth", tag = "reports", params(ReportParams),
    responses((status = 200, body = Data<NetWorthReport>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn net_worth(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(params): ApiQuery<ReportParams>,
) -> Result<Json<Data<NetWorthReport>>, ApiError> {
    let report = state
        .app
        .reports
        .net_worth(&auth.ctx, &params.query()?)
        .await?;
    Ok(Json(Data::new(report)))
}

/// Income (deposits) and expenses (withdrawals) per interval and currency;
/// transfers between own accounts are neither.
#[utoipa::path(get, path = "/reports/cash-flow", tag = "reports", params(ReportParams),
    responses((status = 200, body = Data<CashFlowReport>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn cash_flow(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(params): ApiQuery<ReportParams>,
) -> Result<Json<Data<CashFlowReport>>, ApiError> {
    let report = state
        .app
        .reports
        .cash_flow(&auth.ctx, &params.query()?)
        .await?;
    Ok(Json(Data::new(report)))
}

/// Income and expenses per category.
#[utoipa::path(get, path = "/reports/by-category", tag = "reports", params(ReportParams),
    responses((status = 200, body = Data<BreakdownReport>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn by_category(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(params): ApiQuery<ReportParams>,
) -> Result<Json<Data<BreakdownReport>>, ApiError> {
    let report = state
        .app
        .reports
        .by_category(&auth.ctx, &params.query()?)
        .await?;
    Ok(Json(Data::new(report)))
}

/// Income and expenses per tag; a split counts once for each of its tags.
#[utoipa::path(get, path = "/reports/by-tag", tag = "reports", params(ReportParams),
    responses((status = 200, body = Data<BreakdownReport>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn by_tag(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(params): ApiQuery<ReportParams>,
) -> Result<Json<Data<BreakdownReport>>, ApiError> {
    let report = state
        .app
        .reports
        .by_tag(&auth.ctx, &params.query()?)
        .await?;
    Ok(Json(Data::new(report)))
}

/// Per budget: limits starting in each interval and withdrawals booked on it.
#[utoipa::path(get, path = "/reports/budget-vs-actual", tag = "reports", params(ReportParams),
    responses((status = 200, body = Data<BudgetReport>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn budget_vs_actual(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(params): ApiQuery<ReportParams>,
) -> Result<Json<Data<BudgetReport>>, ApiError> {
    let report = state
        .app
        .reports
        .budget_vs_actual(&auth.ctx, &params.query()?)
        .await?;
    Ok(Json(Data::new(report)))
}
