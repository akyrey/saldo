//! `/recurrences`: CRUD, firing, and a preview of upcoming dates.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::NaiveDate;
use saldo_app::ctx::Origin;
use saldo_app::recurrences::{
    RecurrenceFiredView, RecurrenceInput, RecurrenceView, RepetitionInput, ScheduleInput,
    TemplateSplitInput,
};
use saldo_core::{AccountId, BillId, BudgetId, CategoryId, PiggyBankId, RecurrenceId, TagId};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::page;
use crate::auth::Auth;
use crate::envelope::{Data, List};
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;

/// Dates a preview returns unless `count` says otherwise.
const DEFAULT_PREVIEW: usize = 5;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_recurrences, create_recurrence))
        .routes(routes!(preview_recurrence))
        .routes(routes!(
            get_recurrence,
            replace_recurrence,
            delete_recurrence
        ))
        .routes(routes!(fire_recurrence))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RecurrenceQuery {
    pub active: Option<bool>,
    pub page: Option<u32>,
    /// At most 200; default 50.
    pub per_page: Option<u32>,
}

#[utoipa::path(get, path = "/recurrences", tag = "recurrences", params(RecurrenceQuery),
    responses((status = 200, body = List<RecurrenceView>), (status = 401, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn list_recurrences(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiQuery(query): ApiQuery<RecurrenceQuery>,
) -> Result<Json<List<RecurrenceView>>, ApiError> {
    let result = state
        .app
        .recurrences
        .list(&auth.ctx, query.active, page(query.page, query.per_page)?)
        .await?;
    Ok(Json(result.into()))
}

/// When a recurrence fires.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RepetitionRequest {
    /// `daily`, `weekly`, `monthly`, `nth_weekday` or `yearly`.
    pub kind: String,
    /// `''` daily; `1`-`7` (Mon-Sun) weekly; `1`-`31` monthly;
    /// `week,weekday` (e.g. `2,3`) nth_weekday; `MM-DD` yearly.
    #[serde(default)]
    pub moment: String,
    /// `1` = every second date, and so on.
    #[serde(default)]
    pub skip: u32,
    /// `keep` (default), `skip`, `previous_friday` or `next_monday`.
    pub weekend_policy: Option<String>,
}

/// One split of the transaction each firing creates.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TemplateSplitRequest {
    pub description: String,
    pub source_id: Uuid,
    pub destination_id: Uuid,
    /// Positive decimal string.
    pub amount: String,
    pub currency: Option<String>,
    pub foreign_amount: Option<String>,
    pub foreign_currency: Option<String>,
    pub category_id: Option<Uuid>,
    pub budget_id: Option<Uuid>,
    pub bill_id: Option<Uuid>,
    /// A transfer into (out of) a linked account adds to (removes from) it.
    pub piggy_bank_id: Option<Uuid>,
    #[serde(default)]
    pub tag_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RecurrenceRequest {
    pub title: String,
    pub description: Option<String>,
    pub first_date: NaiveDate,
    pub repeat_until: Option<NaiveDate>,
    pub max_occurrences: Option<u32>,
    /// Default true.
    pub apply_rules: Option<bool>,
    /// Default true.
    pub active: Option<bool>,
    pub notes: Option<String>,
    pub repetitions: Vec<RepetitionRequest>,
    pub splits: Vec<TemplateSplitRequest>,
}

fn repetitions(list: Vec<RepetitionRequest>) -> Vec<RepetitionInput> {
    list.into_iter()
        .map(|r| RepetitionInput {
            kind: r.kind,
            moment: r.moment,
            skip: r.skip,
            weekend_policy: r.weekend_policy,
        })
        .collect()
}

impl From<RecurrenceRequest> for RecurrenceInput {
    fn from(body: RecurrenceRequest) -> Self {
        let splits = body
            .splits
            .into_iter()
            .map(|s| TemplateSplitInput {
                description: s.description,
                source_id: AccountId(s.source_id),
                destination_id: AccountId(s.destination_id),
                amount: s.amount,
                currency: s.currency,
                foreign_amount: s.foreign_amount,
                foreign_currency: s.foreign_currency,
                category_id: s.category_id.map(CategoryId),
                budget_id: s.budget_id.map(BudgetId),
                bill_id: s.bill_id.map(BillId),
                piggy_bank_id: s.piggy_bank_id.map(PiggyBankId),
                tag_ids: s.tag_ids.into_iter().map(TagId).collect(),
            })
            .collect();
        RecurrenceInput {
            title: body.title,
            description: body.description,
            first_date: body.first_date,
            repeat_until: body.repeat_until,
            max_occurrences: body.max_occurrences,
            apply_rules: body.apply_rules,
            active: body.active,
            notes: body.notes,
            repetitions: repetitions(body.repetitions),
            splits,
        }
    }
}

/// Creates a recurrence. The template splits must pass the ledger rules.
#[utoipa::path(post, path = "/recurrences", tag = "recurrences", request_body = RecurrenceRequest,
    responses((status = 201, body = Data<RecurrenceView>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn create_recurrence(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<RecurrenceRequest>,
) -> Result<(StatusCode, Json<Data<RecurrenceView>>), ApiError> {
    let view = state
        .app
        .recurrences
        .create(&auth.ctx, Origin::Api, body.into())
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[utoipa::path(get, path = "/recurrences/{id}", tag = "recurrences", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<RecurrenceView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_recurrence(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<RecurrenceView>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .recurrences
            .get(&auth.ctx, RecurrenceId(id))
            .await?,
    )))
}

/// Replaces the whole recurrence (schedule and template).
#[utoipa::path(put, path = "/recurrences/{id}", tag = "recurrences", params(("id" = Uuid, Path)), request_body = RecurrenceRequest,
    responses((status = 200, body = Data<RecurrenceView>), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn replace_recurrence(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<RecurrenceRequest>,
) -> Result<Json<Data<RecurrenceView>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .recurrences
            .replace(&auth.ctx, Origin::Api, RecurrenceId(id), body.into())
            .await?,
    )))
}

/// Deletes the recurrence; transactions it created stay.
#[utoipa::path(delete, path = "/recurrences/{id}", tag = "recurrences", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_recurrence(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .recurrences
        .delete(&auth.ctx, Origin::Api, RecurrenceId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct FireRequest {
    /// Defaults to the household's today.
    pub date: Option<NaiveDate>,
}

/// Creates the transaction for `date` now. Idempotent: a second call for
/// the same date returns the existing group with 200 and `created: false`.
#[utoipa::path(post, path = "/recurrences/{id}/fire", tag = "recurrences", params(("id" = Uuid, Path)), request_body = FireRequest,
    responses((status = 201, body = Data<RecurrenceFiredView>), (status = 200, body = Data<RecurrenceFiredView>),
        (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn fire_recurrence(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<FireRequest>,
) -> Result<(StatusCode, Json<Data<RecurrenceFiredView>>), ApiError> {
    let date = match body.date {
        Some(date) => date,
        None => state.app.identity.household_today(&auth.ctx).await?,
    };
    let view = state
        .app
        .recurrences
        .fire(&auth.ctx, Origin::Api, RecurrenceId(id), date)
        .await?;
    let status = if view.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(Data::new(view))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PreviewRequest {
    pub first_date: NaiveDate,
    pub repeat_until: Option<NaiveDate>,
    pub max_occurrences: Option<u32>,
    pub repetitions: Vec<RepetitionRequest>,
    /// How many dates (default 5, at most 100).
    pub count: Option<usize>,
}

/// The next dates a schedule fires on, from the household's today, without
/// saving anything (for the editor's preview).
#[utoipa::path(post, path = "/recurrences/preview", tag = "recurrences", request_body = PreviewRequest,
    responses((status = 200, body = Data<Vec<NaiveDate>>), (status = 422, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn preview_recurrence(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<PreviewRequest>,
) -> Result<Json<Data<Vec<NaiveDate>>>, ApiError> {
    let schedule = ScheduleInput {
        first_date: body.first_date,
        repeat_until: body.repeat_until,
        max_occurrences: body.max_occurrences,
        repetitions: repetitions(body.repetitions),
    };
    let count = body.count.unwrap_or(DEFAULT_PREVIEW);
    let dates = state
        .app
        .recurrences
        .preview(&auth.ctx, &schedule, count)
        .await?;
    Ok(Json(Data::new(dates)))
}
