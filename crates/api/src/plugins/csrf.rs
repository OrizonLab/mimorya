//! CSRF protection: state-changing requests must carry a custom header (not
//! settable cross-site without CORS, which is disabled) and, when present, an
//! `Origin` matching the web application. A SAML provider's response alone
//! is posted from its own site, without it: the sealed attempt in its own
//! cookie and the signed response bind it instead (ADR-068).

use axum::extract::{Request, State};
use axum::http::Method;
use axum::http::header::ORIGIN;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use mimorya_core::DomainError;

use crate::app::AppState;
use crate::http::ApiError;
use crate::routes::SAML_ACS_ROUTE;

pub const REQUEST_HEADER: &str = "x-mimorya-request";

pub async fn csrf_protection(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) || request.uri().path() == SAML_ACS_ROUTE
    {
        return next.run(request).await;
    }
    let headers = request.headers();
    if headers.get(REQUEST_HEADER).and_then(|v| v.to_str().ok()) != Some("1") {
        return ApiError::from(DomainError::forbidden("En-tête de requête manquant"))
            .into_response();
    }
    if let Some(origin) = headers.get(ORIGIN)
        && origin.to_str().ok() != Some(state.config.web_origin.as_str())
    {
        return ApiError::from(DomainError::forbidden("Origine non autorisée")).into_response();
    }
    next.run(request).await
}
