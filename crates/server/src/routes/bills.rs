//! `/bills` and `/bill-periods`: bills, their periods, and paying them.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::NaiveDate;
use saldo_app::bills::{BillHistoryView, BillPeriodView, BillUpdate, BillView, NewBill};
use saldo_app::ctx::Origin;
use saldo_core::{BillId, BillPeriodId, SplitId};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::{double_option, page};
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_bills, create_bill))
        .routes(routes!(get_bill, update_bill, delete_bill))
        .routes(routes!(bill_periods))
        .routes(routes!(bill_history))
        .routes(routes!(pay_period))
        .routes(routes!(skip_period))
        .routes(routes!(reopen_period))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BillQuery {
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

/// Bills with their next open due date.
#[utoipa::path(get, path = "/bills", tag = "bills", params(BillQuery),
    responses((status = 200, body = List<BillView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_bills(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<BillQuery>,
) -> Result<Json<List<BillView>>, ApiError> {
    Ok(Json(
        state
            .app
            .bills
            .list(&auth.ctx, page(query.page, query.per_page)?)
            .await?
            .into(),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBillRequest {
    pub name: String,
    /// Decimal strings; `amount_min <= amount_max`.
    pub amount_min: String,
    pub amount_max: String,
    /// Defaults to the household's primary currency.
    pub currency: Option<String>,
    pub first_due_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    /// `weekly`, `monthly`, `quarterly`, `half_year` or `yearly`.
    pub repeat_freq: String,
    /// Periods left out between due dates (1 on monthly = every 2 months).
    pub skip: Option<u32>,
    /// Days before the due date of the first reminder (default 3).
    pub remind_days_before: Option<u32>,
    /// Days between overdue reminders (default 1).
    pub remind_every_days: Option<u32>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    pub notes: Option<String>,
}

/// Creates a bill and its periods up to 60 days ahead.
#[utoipa::path(post, path = "/bills", tag = "bills", request_body = CreateBillRequest,
    responses((status = 201, body = Data<BillView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_bill(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<CreateBillRequest>,
) -> Result<(StatusCode, Json<Data<BillView>>), ApiError> {
    let input = NewBill {
        name: body.name,
        amount_min: body.amount_min,
        amount_max: body.amount_max,
        currency: body.currency,
        first_due_date: body.first_due_date,
        end_date: body.end_date,
        repeat_freq: body.repeat_freq,
        skip: body.skip,
        remind_days_before: body.remind_days_before,
        remind_every_days: body.remind_every_days,
        active: body.active,
        sort_order: body.sort_order,
        notes: body.notes,
    };
    let view = state
        .app
        .bills
        .create(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[utoipa::path(get, path = "/bills/{id}", tag = "bills", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<BillView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_bill(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<BillView>>, ApiError> {
    Ok(Json(Data::new(
        state.app.bills.get(&auth.ctx, BillId(id)).await?,
    )))
}

/// Fields to change; `null` clears `end_date` or `notes`. The currency is fixed.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct BillPatch {
    pub name: Option<String>,
    pub amount_min: Option<String>,
    pub amount_max: Option<String>,
    pub first_due_date: Option<NaiveDate>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<NaiveDate>)]
    pub end_date: Option<Option<NaiveDate>>,
    pub repeat_freq: Option<String>,
    pub skip: Option<u32>,
    pub remind_days_before: Option<u32>,
    pub remind_every_days: Option<u32>,
    pub active: Option<bool>,
    pub sort_order: Option<i64>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub notes: Option<Option<String>>,
}

/// Changes a bill. Open, never-reminded periods from today on follow a new schedule.
#[utoipa::path(patch, path = "/bills/{id}", tag = "bills", params(("id" = Uuid, Path)), request_body = BillPatch,
    responses((status = 200, body = Data<BillView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_bill(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<BillPatch>,
) -> Result<Json<Data<BillView>>, ApiError> {
    let update = BillUpdate {
        name: body.name,
        amount_min: body.amount_min,
        amount_max: body.amount_max,
        first_due_date: body.first_due_date,
        end_date: body.end_date,
        repeat_freq: body.repeat_freq,
        skip: body.skip,
        remind_days_before: body.remind_days_before,
        remind_every_days: body.remind_every_days,
        active: body.active,
        sort_order: body.sort_order,
        notes: body.notes,
    };
    Ok(Json(Data::new(
        state
            .app
            .bills
            .update(&auth.ctx, Origin::Api, BillId(id), update)
            .await?,
    )))
}

/// Deletes a bill and its periods; splits keep no bill.
#[utoipa::path(delete, path = "/bills/{id}", tag = "bills", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_bill(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .bills
        .delete(&auth.ctx, Origin::Api, BillId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A bill's periods, newest first.
#[utoipa::path(get, path = "/bills/{id}/periods", tag = "bills", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<Vec<BillPeriodView>>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn bill_periods(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<Vec<BillPeriodView>>>, ApiError> {
    Ok(Json(Data::new(
        state.app.bills.periods(&auth.ctx, BillId(id)).await?,
    )))
}

/// A bill's linked payments (newest first), its due dates over the bill's
/// whole duration (oldest first) and a summary of what was paid.
#[utoipa::path(get, path = "/bills/{id}/history", tag = "bills", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<BillHistoryView>), (status = 401, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn bill_history(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<BillHistoryView>>, ApiError> {
    Ok(Json(Data::new(
        state.app.bills.history(&auth.ctx, BillId(id)).await?,
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PayRequest {
    /// The split that paid this period.
    pub split_id: Uuid,
}

/// Marks a period paid by a split (`bill.paid`).
#[utoipa::path(post, path = "/bill-periods/{id}/pay", tag = "bills", params(("id" = Uuid, Path)), request_body = PayRequest,
    responses((status = 200, body = Data<BillPeriodView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn pay_period(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<PayRequest>,
) -> Result<Json<Data<BillPeriodView>>, ApiError> {
    let view = state
        .app
        .bills
        .pay(
            &auth.ctx,
            Origin::Api,
            BillPeriodId(id),
            SplitId(body.split_id),
        )
        .await?;
    Ok(Json(Data::new(view)))
}

/// Marks an open period as not needing payment.
#[utoipa::path(post, path = "/bill-periods/{id}/skip", tag = "bills", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<BillPeriodView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn skip_period(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<BillPeriodView>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .bills
            .skip(&auth.ctx, Origin::Api, BillPeriodId(id))
            .await?,
    )))
}

/// Opens a paid or skipped period again (`bill.period_reopened`).
#[utoipa::path(post, path = "/bill-periods/{id}/reopen", tag = "bills", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<BillPeriodView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn reopen_period(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<BillPeriodView>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .bills
            .reopen(&auth.ctx, Origin::Api, BillPeriodId(id))
            .await?,
    )))
}
