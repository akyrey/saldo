//! Phase 10 reports: by counterparty, year over year, category averages.

use axum::Json;
use axum::extract::State;
use saldo_app::error::AppError;
use saldo_app::reports::{
    BreakdownReport, CategoryAveragesReport, DEFAULT_AVERAGE_MONTHS, Metric, YearOverYearQuery,
    YearOverYearReport,
};
use saldo_app::safe_to_spend::SafeToSpendView;
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::reports::ReportParams;
use super::scope;
use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::ApiQuery;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(by_counterparty))
        .routes(routes!(year_over_year))
        .routes(routes!(category_averages))
        .routes(routes!(safe_to_spend))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SafeToSpendParams {
    /// `household` (default), `mine` or `user:<id>`.
    pub scope: Option<String>,
}

/// This month's income (received and planned) minus what was spent and what
/// is committed: bills, planned withdrawals, what budgets have left, piggy
/// bank targets and transfers into savings. Primary currency only.
#[utoipa::path(get, path = "/reports/safe-to-spend", tag = "reports", params(SafeToSpendParams),
    responses((status = 200, body = Data<SafeToSpendView>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn safe_to_spend(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(params): ApiQuery<SafeToSpendParams>,
) -> Result<Json<Data<SafeToSpendView>>, ApiError> {
    let view = state
        .app
        .safe_to_spend
        .current(&auth.ctx, scope(params.scope.as_deref())?)
        .await?;
    Ok(Json(Data::new(view)))
}

/// Expenses per expense account and income per revenue account.
#[utoipa::path(get, path = "/reports/by-counterparty", tag = "reports", params(ReportParams),
    responses((status = 200, body = Data<BreakdownReport>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn by_counterparty(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(params): ApiQuery<ReportParams>,
) -> Result<Json<Data<BreakdownReport>>, ApiError> {
    let report = state
        .app
        .reports
        .by_counterparty(&auth.ctx, &params.query()?)
        .await?;
    Ok(Json(Data::new(report)))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct YearOverYearParams {
    /// 2 to 5 (default 3), ending with the current year.
    pub years: Option<u32>,
    /// `expenses` (default), `income` or `savings`.
    pub metric: Option<String>,
    /// One category, or `none` for splits without one.
    pub category_id: Option<String>,
    /// `household` (default), `mine` or `user:<id>`.
    pub scope: Option<String>,
}

/// One amount per month for each of the last few years, with the change
/// against the same months a year earlier.
#[utoipa::path(get, path = "/reports/year-over-year", tag = "reports", params(YearOverYearParams),
    responses((status = 200, body = Data<YearOverYearReport>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn year_over_year(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(params): ApiQuery<YearOverYearParams>,
) -> Result<Json<Data<YearOverYearReport>>, ApiError> {
    let category = match params.category_id.as_deref().map(str::trim) {
        None | Some("") => None,
        Some("none") => Some(None),
        Some(id) => {
            let id: Uuid = id
                .parse()
                .map_err(|_| AppError::invalid("category_id", "must be an ID or none"))?;
            Some(Some(id.to_string()))
        }
    };
    let query = YearOverYearQuery {
        years: params.years.unwrap_or(3),
        metric: params
            .metric
            .as_deref()
            .map_or(Ok(Metric::default()), Metric::parse)?,
        category,
        scope: scope(params.scope.as_deref())?,
    };
    let today = state.app.identity.household_today(&auth.ctx).await?;
    let report = state
        .app
        .reports
        .year_over_year(&auth.ctx, today, &query)
        .await?;
    Ok(Json(Data::new(report)))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CategoryAveragesParams {
    /// Complete months before the current one: 3 to 24 (default 12).
    pub months: Option<u32>,
    /// `household` (default), `mine` or `user:<id>`.
    pub scope: Option<String>,
}

/// Average, smallest and largest monthly spending per category, and this
/// month so far against the average.
#[utoipa::path(get, path = "/reports/category-averages", tag = "reports", params(CategoryAveragesParams),
    responses((status = 200, body = Data<CategoryAveragesReport>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn category_averages(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(params): ApiQuery<CategoryAveragesParams>,
) -> Result<Json<Data<CategoryAveragesReport>>, ApiError> {
    let today = state.app.identity.household_today(&auth.ctx).await?;
    let report = state
        .app
        .reports
        .category_averages(
            &auth.ctx,
            today,
            params.months.unwrap_or(DEFAULT_AVERAGE_MONTHS),
            scope(params.scope.as_deref())?,
        )
        .await?;
    Ok(Json(Data::new(report)))
}
