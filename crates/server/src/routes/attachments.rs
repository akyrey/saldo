//! `/attachments`: upload (multipart), metadata, download, delete.

use axum::Json;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use saldo_app::attachments::{AttachmentView, MAX_SIZE, NewAttachment};
use saldo_app::ctx::Origin;
use saldo_app::error::AppError;
use saldo_core::AttachmentId;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Auth;
use crate::envelope::Data;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::ApiPath;
use crate::state::AppState;

/// Room for the form fields around the largest accepted file.
const BODY_LIMIT: usize = MAX_SIZE + 64 * 1024;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(upload_attachment))
        .routes(routes!(get_attachment, delete_attachment))
        .routes(routes!(download_attachment))
        // Axum's default body limit is 2 MB; attachments may be larger.
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

/// The multipart form (documentation only; the handler reads the parts).
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct UploadForm {
    /// `split`, `account` or `tag`.
    attachable_type: String,
    attachable_id: Uuid,
    title: Option<String>,
    notes: Option<String>,
    /// The file; its part's file name and content type are kept.
    #[schema(value_type = String, format = Binary)]
    file: Vec<u8>,
}

/// Uploads a file (at most 20 MiB; PDF, images, text, CSV, XML).
#[utoipa::path(post, path = "/attachments", tag = "attachments",
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    responses((status = 201, body = Data<AttachmentView>), (status = 400, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn upload_attachment(
    State(state): State<AppState>,
    Auth(auth): Auth,
    multipart: Multipart,
) -> Result<(StatusCode, Json<Data<AttachmentView>>), ApiError> {
    let input = read_form(multipart).await?;
    let view = state
        .app
        .attachments
        .upload(&auth.ctx, Origin::Api, input)
        .await?;
    Ok((StatusCode::CREATED, Json(Data::new(view))))
}

async fn read_form(mut multipart: Multipart) -> Result<NewAttachment, ApiError> {
    let bad = |e: axum::extract::multipart::MultipartError| ApiError::BadRequest(e.body_text());
    let mut input = NewAttachment {
        attachable_type: String::new(),
        attachable_id: String::new(),
        filename: String::new(),
        title: None,
        notes: None,
        mime_type: String::new(),
        bytes: Vec::new(),
    };
    while let Some(field) = multipart.next_field().await.map_err(bad)? {
        match field.name().unwrap_or_default() {
            "attachable_type" => input.attachable_type = field.text().await.map_err(bad)?,
            "attachable_id" => input.attachable_id = field.text().await.map_err(bad)?,
            "title" => {
                input.title =
                    Some(field.text().await.map_err(bad)?).filter(|t| !t.trim().is_empty())
            }
            "notes" => {
                input.notes =
                    Some(field.text().await.map_err(bad)?).filter(|t| !t.trim().is_empty())
            }
            "file" => {
                input.filename = field.file_name().unwrap_or_default().to_string();
                input.mime_type = field
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                input.bytes = field.bytes().await.map_err(bad)?.to_vec();
            }
            _ => {}
        }
    }
    if input.bytes.is_empty() && input.filename.is_empty() {
        return Err(AppError::invalid("file", "a file part is required").into());
    }
    Ok(input)
}

#[utoipa::path(get, path = "/attachments/{id}", tag = "attachments", params(("id" = Uuid, Path)),
    responses((status = 200, body = Data<AttachmentView>), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn get_attachment(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Data<AttachmentView>>, ApiError> {
    Ok(Json(Data::new(
        state
            .app
            .attachments
            .get(&auth.ctx, AttachmentId(id))
            .await?,
    )))
}

/// The file itself, always as a download (never rendered inline).
#[utoipa::path(get, path = "/attachments/{id}/download", tag = "attachments", params(("id" = Uuid, Path)),
    responses((status = 200, description = "The file", content_type = "application/octet-stream"), (status = 404, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn download_attachment(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Response, ApiError> {
    let (view, bytes) = state
        .app
        .attachments
        .download(&auth.ctx, AttachmentId(id))
        .await?;
    // Keep the file name to safe characters for the header.
    let name: String = view
        .filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || ".-_ ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{name}\""))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"));
    let content_type = HeaderValue::from_str(&view.mime_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, content_type);
    headers.insert(header::CONTENT_DISPOSITION, disposition);
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

#[utoipa::path(delete, path = "/attachments/{id}", tag = "attachments", params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ErrorEnvelope)), security(("bearer" = []), ("cookie" = [])))]
pub async fn delete_attachment(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .app
        .attachments
        .delete(&auth.ctx, Origin::Api, AttachmentId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
