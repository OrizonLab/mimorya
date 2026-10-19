//! Mapping of failures to HTTP responses. Domain errors keep their code and
//! French message; anything else is logged and answered with a generic
//! message that reveals nothing about the internals.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mimorya_core::{DomainError, ErrorCode};
use serde_json::{Value, json};

#[derive(Debug)]
pub enum ApiError {
    Domain(DomainError),
    /// Malformed request outside the domain rules (content type, body size…).
    Request {
        status: StatusCode,
        message: String,
    },
    RateLimited,
    /// A limit whose window or way out differs from the per-minute ones.
    Throttled(&'static str),
    Internal(anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn request(status: StatusCode, message: impl Into<String>) -> Self {
        Self::Request {
            status,
            message: message.into(),
        }
    }

    /// Invalid input, with the issues found (field path and message).
    pub fn invalid(issues: Vec<String>) -> Self {
        let issues: Vec<Value> = issues
            .into_iter()
            .map(|issue| match issue.split_once(" : ") {
                Some((path, message)) => json!({ "path": path, "message": message }),
                None => json!({ "path": "", "message": issue }),
            })
            .collect();
        Self::Domain(
            DomainError::validation("Requête invalide").with_details(json!({ "issues": issues })),
        )
    }
}

impl From<DomainError> for ApiError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        match error.downcast::<DomainError>() {
            Ok(domain) => Self::Domain(domain),
            Err(other) => Self::Internal(other),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

fn status_of(code: ErrorCode) -> StatusCode {
    match code {
        ErrorCode::Validation => StatusCode::BAD_REQUEST,
        ErrorCode::Unauthenticated => StatusCode::UNAUTHORIZED,
        ErrorCode::Forbidden => StatusCode::FORBIDDEN,
        ErrorCode::NotFound => StatusCode::NOT_FOUND,
        ErrorCode::Conflict => StatusCode::CONFLICT,
        ErrorCode::BudgetExceeded => StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn body(code: Value, message: &str, details: Option<Value>) -> Json<Value> {
    let mut error = json!({ "code": code, "message": message });
    if let Some(details) = details {
        error["details"] = details;
    }
    Json(json!({ "error": error }))
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Domain(error) => (
                status_of(error.code),
                body(json!(error.code), &error.message, error.details),
            )
                .into_response(),
            Self::Request { status, message } => {
                (status, body(json!("request"), &message, None)).into_response()
            }
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                body(
                    json!("rate_limited"),
                    "Trop de requêtes : réessayez dans une minute.",
                    None,
                ),
            )
                .into_response(),
            Self::Throttled(message) => (
                StatusCode::TOO_MANY_REQUESTS,
                body(json!("rate_limited"), message, None),
            )
                .into_response(),
            Self::Internal(error) => {
                tracing::error!(error = format!("{error:#}"), "Unhandled error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    body(json!("internal"), "Erreur interne", None),
                )
                    .into_response()
            }
        }
    }
}
