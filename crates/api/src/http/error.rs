//! Mapping of failures to HTTP responses. Domain errors keep their code and
//! French message; anything else is logged and answered with a generic
//! message that reveals nothing about the internals.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use mimorya_core::{DomainError, ErrorCode, UnknownReference};
use serde_json::{Value, json};

use crate::contract::{ErrorBody, ErrorDetail, ErrorKind};

/// Carried by the response of a refusal (`Forbidden`), of a resource
/// answered as absent (`NotFound`) or of an identifier sent in the body that
/// designates nothing the company has (`UnknownReference`, answered as an
/// invalid request), with its message, for the layers that record them, such
/// as the administration's (ADR-038, ADR-047, ADR-054). Never sent: the
/// message is already in the body, and the identifier is what was sent.
#[derive(Debug, Clone)]
pub enum Refusal {
    Forbidden(String),
    NotFound(String),
    UnknownReference(String, UnknownReference),
}

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

fn body(code: ErrorKind, message: &str, details: Option<Value>) -> Json<ErrorBody> {
    Json(ErrorBody {
        error: ErrorDetail {
            code,
            message: message.into(),
            details,
        },
    })
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Domain(error) => {
                let message = error.message.clone();
                let refusal = match (error.code, error.reference) {
                    (_, Some(reference)) => Some(Refusal::UnknownReference(message, *reference)),
                    (ErrorCode::Forbidden, None) => Some(Refusal::Forbidden(message)),
                    (ErrorCode::NotFound, None) => Some(Refusal::NotFound(message)),
                    _ => None,
                };
                let mut response = (
                    status_of(error.code),
                    body(error.code.into(), &error.message, error.details),
                )
                    .into_response();
                if let Some(refusal) = refusal {
                    response.extensions_mut().insert(refusal);
                }
                response
            }
            Self::Request { status, message } => {
                (status, body(ErrorKind::Request, &message, None)).into_response()
            }
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                body(
                    ErrorKind::RateLimited,
                    "Trop de requêtes : réessayez dans une minute.",
                    None,
                ),
            )
                .into_response(),
            Self::Throttled(message) => (
                StatusCode::TOO_MANY_REQUESTS,
                body(ErrorKind::RateLimited, message, None),
            )
                .into_response(),
            Self::Internal(error) => {
                tracing::error!(error = format!("{error:#}"), "Unhandled error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    body(ErrorKind::Internal, "Erreur interne", None),
                )
                    .into_response()
            }
        }
    }
}
