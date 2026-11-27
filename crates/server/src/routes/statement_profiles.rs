//! `/statement-profiles`: how to read one bank's export (Phase 13), the
//! built-in presets, and a preview that saves nothing.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::StatusCode;
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_app::statements::parse::{MAX_FILE_BYTES, Mapping};
use saldo_app::statements::{
    Preset, PreviewView, ProfileInput, ProfileView, TransferRule, presets,
};
use serde::Deserialize;
use serde_json::Value;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::{ApiJson, ApiPath};
use crate::state::AppState;

/// Room for the form fields around the largest accepted file.
pub(crate) const BODY_LIMIT: usize = MAX_FILE_BYTES + 64 * 1024;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_profiles, create_profile))
        .routes(routes!(get_profile, update_profile, delete_profile))
        .routes(routes!(list_presets))
        .routes(routes!(preview_profile))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TransferRuleRequest {
    /// Text the line's description contains (ignoring case).
    pub pattern: String,
    /// The household's own asset or liability accounts.
    pub account_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ProfileRequest {
    pub name: String,
    /// The asset or liability account files are imported into by default.
    pub account_id: Option<Uuid>,
    /// `csv` or `xlsx`.
    pub format: String,
    /// The column mapping, as a preset returns it.
    #[schema(value_type = Object)]
    pub mapping: Value,
    /// Part column header (for example "Meal Vouchers") → asset account,
    /// or null to keep that part as a note.
    #[serde(default)]
    pub part_accounts: BTreeMap<String, Option<Uuid>>,
    #[serde(default)]
    pub transfer_rules: Vec<TransferRuleRequest>,
}

impl ProfileRequest {
    fn into_input(self) -> Result<ProfileInput, AppError> {
        let mapping = mapping_of(self.mapping)?;
        Ok(ProfileInput {
            name: self.name,
            account_id: self.account_id.map(|id| id.to_string()),
            format: self.format,
            mapping,
            part_accounts: self
                .part_accounts
                .into_iter()
                .map(|(column, account)| (column, account.map(|id| id.to_string())))
                .collect(),
            transfer_rules: self
                .transfer_rules
                .into_iter()
                .map(|rule| TransferRule {
                    pattern: rule.pattern,
                    account_ids: rule.account_ids.iter().map(Uuid::to_string).collect(),
                })
                .collect(),
        })
    }
}

fn mapping_of(value: Value) -> Result<Mapping, AppError> {
    serde_json::from_value(value).map_err(|error| AppError::invalid("mapping", error.to_string()))
}

/// The household's profiles by name (unpaginated: a household has a few).
#[utoipa::path(get, path = "/statement-profiles", tag = "statements",
    responses((status = 200, body = Data<Vec<ProfileView>>)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_profiles(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Data<Vec<ProfileView>>>, ApiError> {
    Ok(Json(Data::new(
        state.app.statements.list_profiles(&auth.ctx).await?,
    )))
}

#[utoipa::path(post, path = "/statement-profiles", tag = "statements", request_body = ProfileRequest,
    responses((status = 201, body = Data<ProfileView>), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn create_profile(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiJson(body): ApiJson<ProfileRequest>,
) -> Result<(StatusCode, Json<Data<ProfileView>>), ApiError> {
    let view = state
        .app
        .statements
        .create_profile(&auth.ctx, Origin::Api, body.into_input()?)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

#[utoipa::path(get, path = "/statement-profiles/{id}", tag = "statements", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<ProfileView>), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn get_profile(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<ProfileView>>, ApiError> {
    let view = state
        .app
        .statements
        .get_profile(&auth.ctx, &id.to_string())
        .await?;
    Ok(Json(Data::new(view)))
}

/// Replaces every field of a profile.
#[utoipa::path(put, path = "/statement-profiles/{id}", tag = "statements", params(("id" = Uuid, Path)),
    request_body = ProfileRequest,
    responses((status = 200, body = Data<ProfileView>), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn update_profile(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<ProfileRequest>,
) -> Result<Json<Data<ProfileView>>, ApiError> {
    let view = state
        .app
        .statements
        .update_profile(&auth.ctx, Origin::Api, &id.to_string(), body.into_input()?)
        .await?;
    Ok(Json(Data::new(view)))
}

/// Deletes a profile; imports made with it keep their lines.
#[utoipa::path(delete, path = "/statement-profiles/{id}", tag = "statements", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_profile(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .statements
        .delete_profile(&auth.ctx, Origin::Api, &id.to_string())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The built-in mappings for Widiba, Mediobanca and Satispay.
#[utoipa::path(get, path = "/statement-profiles/presets", tag = "statements",
    responses((status = 200, body = Data<Vec<Preset>>)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn list_presets(Auth(_auth): Auth) -> Json<Data<Vec<Preset>>> {
    Json(Data::new(presets()))
}

/// The preview form (documentation only; the handler reads the parts).
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct PreviewForm {
    /// `csv` or `xlsx`.
    format: String,
    /// The mapping as JSON text.
    mapping: String,
    /// The account whose currency amounts are read in; the household's
    /// primary currency when absent.
    account_id: Option<Uuid>,
    #[schema(value_type = String, format = Binary)]
    file: Vec<u8>,
}

/// Reads a file with a mapping that is not saved yet: the first 20 lines and
/// every row that would not be imported. Saves nothing.
#[utoipa::path(post, path = "/statement-profiles/preview", tag = "statements",
    request_body(content = PreviewForm, content_type = "multipart/form-data"),
    responses((status = 200, body = Data<PreviewView>), (status = 400, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn preview_profile(
    State(state): State<AppState>,
    Auth(auth): Auth,
    multipart: Multipart,
) -> Result<Json<Data<PreviewView>>, ApiError> {
    let form = read_form(multipart).await?;
    let mapping_text = form.text("mapping").unwrap_or_default();
    let mapping: Value = serde_json::from_str(&mapping_text)
        .map_err(|error| AppError::invalid("mapping", error.to_string()))?;
    let view = state
        .app
        .statements
        .preview(
            &auth.ctx,
            &form.text("format").unwrap_or_default(),
            mapping_of(mapping)?,
            form.text("account_id").filter(|id| !id.trim().is_empty()),
            &form.file()?.1,
        )
        .await?;
    Ok(Json(Data::new(view)))
}

/// A multipart form read into its text fields and its one file.
pub(crate) struct Form {
    fields: Vec<(String, String)>,
    file: Option<(String, Vec<u8>)>,
}

impl Form {
    pub(crate) fn text(&self, name: &str) -> Option<String> {
        self.fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value.clone())
    }

    /// `(file name, bytes)`; a 422 on `file` when there is none.
    pub(crate) fn file(&self) -> Result<(String, Vec<u8>), AppError> {
        self.file
            .clone()
            .ok_or_else(|| AppError::invalid("file", "a file part is required"))
    }
}

pub(crate) async fn read_form(mut multipart: Multipart) -> Result<Form, ApiError> {
    let bad = |e: axum::extract::multipart::MultipartError| ApiError::BadRequest(e.body_text());
    let mut form = Form {
        fields: Vec::new(),
        file: None,
    };
    while let Some(field) = multipart.next_field().await.map_err(bad)? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            let filename = field.file_name().unwrap_or_default().to_string();
            let bytes = field.bytes().await.map_err(bad)?.to_vec();
            form.file = Some((filename, bytes));
        } else {
            let value = field.text().await.map_err(bad)?;
            form.fields.push((name, value));
        }
    }
    Ok(form)
}
