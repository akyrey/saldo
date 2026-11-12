//! Maps service errors to HTTP statuses and the API error envelope.

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use saldo_app::error::AppError;
use saldo_core::ValidationError;
use serde::Serialize;
use utoipa::ToSchema;

/// `{ "error": { "code", "message", "details" } }`
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    /// Stable machine-readable code, for example `validation_failed`.
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ErrorDetail>,
}

/// One problem with the input; `field` is a path such as `splits[1].amount`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    pub field: Option<String>,
    pub code: String,
    pub message: String,
}

/// Every way a request can fail.
#[derive(Debug)]
pub enum ApiError {
    App(AppError),
    /// Malformed JSON, query string or path.
    BadRequest(String),
    TooManyRequests,
    /// A cookie-authenticated write without the CSRF header or right `Origin`.
    CsrfFailed,
}

/// Lets handlers write `service_call().await?`: `?` converts the `AppError`.
impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        ApiError::App(error)
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        ApiError::BadRequest(rejection.body_text())
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        ApiError::BadRequest(rejection.body_text())
    }
}

impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        ApiError::BadRequest(rejection.body_text())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body) = self.status_and_body();
        (status, Json(ErrorEnvelope { error: body })).into_response()
    }
}

impl ApiError {
    fn status_and_body(self) -> (StatusCode, ErrorBody) {
        let simple = |code: &str, message: &str| ErrorBody {
            code: code.into(),
            message: message.into(),
            details: vec![],
        };
        match self {
            ApiError::App(AppError::NotFound) => {
                (StatusCode::NOT_FOUND, simple("not_found", "not found"))
            }
            ApiError::App(AppError::Conflict(message)) => {
                (StatusCode::CONFLICT, simple("conflict", &message))
            }
            ApiError::App(AppError::Validation(errors)) => {
                let details = errors.iter().map(validation_detail).collect();
                let body = ErrorBody {
                    details,
                    ..simple("validation_failed", "the transaction breaks a ledger rule")
                };
                (StatusCode::UNPROCESSABLE_ENTITY, body)
            }
            ApiError::App(AppError::Invalid { field, message }) => {
                let detail = ErrorDetail {
                    field: Some(field),
                    code: "invalid".into(),
                    message,
                };
                let body = ErrorBody {
                    details: vec![detail],
                    ..simple("validation_failed", "the request has an invalid field")
                };
                (StatusCode::UNPROCESSABLE_ENTITY, body)
            }
            ApiError::App(AppError::Unauthorized) => (
                StatusCode::UNAUTHORIZED,
                simple("unauthorized", "authentication required"),
            ),
            ApiError::App(AppError::Forbidden) => (
                StatusCode::FORBIDDEN,
                simple("forbidden", "only the household owner can do this"),
            ),
            ApiError::App(error @ (AppError::Database(_) | AppError::Internal(_))) => {
                // Details go to the log only; the client gets a generic message.
                tracing::error!(%error, "request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    simple("internal", "internal server error"),
                )
            }
            ApiError::BadRequest(message) => {
                (StatusCode::BAD_REQUEST, simple("bad_request", &message))
            }
            ApiError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                simple("rate_limited", "too many attempts, try again later"),
            ),
            ApiError::CsrfFailed => (
                StatusCode::FORBIDDEN,
                simple(
                    "csrf_failed",
                    "cookie requests that change data need the X-Saldo-Csrf header and a matching Origin",
                ),
            ),
        }
    }
}

/// A ledger error as a detail, pointing at the field it concerns.
fn validation_detail(error: &ValidationError) -> ErrorDetail {
    let part = match error {
        ValidationError::AmountNotPositive { .. } | ValidationError::CurrencyMismatch { .. } => {
            Some("amount")
        }
        ValidationError::ForeignAmountNotPositive { .. }
        | ValidationError::ForeignCurrencySameAsAmount { .. }
        | ValidationError::ForeignAmountRequired { .. }
        | ValidationError::ForeignCurrencyMismatch { .. } => Some("foreign_amount"),
        _ => None,
    };
    let field = error.split().map(|index| match part {
        Some(part) => format!("splits[{index}].{part}"),
        None => format!("splits[{index}]"),
    });
    ErrorDetail {
        field,
        code: error.code().to_string(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status_of(error: ApiError) -> (u16, String) {
        let (status, body) = error.status_and_body();
        (status.as_u16(), body.code)
    }

    #[test]
    fn maps_every_error_to_a_status_and_code() {
        let cases = [
            (ApiError::App(AppError::NotFound), 404, "not_found"),
            (
                ApiError::App(AppError::Conflict("x".into())),
                409,
                "conflict",
            ),
            (
                ApiError::App(AppError::Validation(vec![])),
                422,
                "validation_failed",
            ),
            (
                ApiError::App(AppError::invalid("f", "m")),
                422,
                "validation_failed",
            ),
            (ApiError::App(AppError::Unauthorized), 401, "unauthorized"),
            (ApiError::App(AppError::Forbidden), 403, "forbidden"),
            (
                ApiError::App(AppError::Internal("secret detail".into())),
                500,
                "internal",
            ),
            (ApiError::BadRequest("bad".into()), 400, "bad_request"),
            (ApiError::TooManyRequests, 429, "rate_limited"),
            (ApiError::CsrfFailed, 403, "csrf_failed"),
        ];
        for (error, status, code) in cases {
            assert_eq!(status_of(error), (status, code.to_string()));
        }
    }

    #[test]
    fn internal_errors_hide_their_details() {
        let (_, body) = ApiError::App(AppError::Internal("secret detail".into())).status_and_body();
        assert!(!body.message.contains("secret"));
    }

    #[test]
    fn ledger_errors_point_at_their_field() {
        let detail = validation_detail(&ValidationError::AmountNotPositive { split: 1 });
        assert_eq!(detail.field.as_deref(), Some("splits[1].amount"));
        assert_eq!(detail.code, "amount_not_positive");
        let detail = validation_detail(&ValidationError::ForeignAmountNotPositive { split: 0 });
        assert_eq!(detail.field.as_deref(), Some("splits[0].foreign_amount"));
        let detail = validation_detail(&ValidationError::SameAccount { split: 2 });
        assert_eq!(detail.field.as_deref(), Some("splits[2]"));
        assert_eq!(validation_detail(&ValidationError::EmptyGroup).field, None);
    }
}
