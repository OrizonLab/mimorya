//! Request extractors that turn malformed input into the API's error format
//! (French messages) instead of axum's plain-text rejections. Bodies and
//! query strings are validated with `validator` before reaching a handler.

use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Path, Request};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::http::request::Parts;
use mimorya_core::tools::validation_issues;
use serde::de::DeserializeOwned;
use uuid::Uuid;
use validator::Validate;

use super::error::ApiError;

fn validate<T: Validate>(value: T) -> Result<T, ApiError> {
    value
        .validate()
        .map_err(|errors| ApiError::invalid(validation_issues(&errors)))?;
    Ok(value)
}

/// JSON body, validated. An empty body reads as `{}`.
pub struct Body<T>(pub T);

impl<S, T> FromRequest<S> for Body<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let is_json = request
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.trim_start().starts_with("application/json"));
        let bytes = Bytes::from_request(request, state).await.map_err(|_| {
            ApiError::request(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Corps de requête trop volumineux",
            )
        })?;
        let bytes = if bytes.is_empty() {
            Bytes::from_static(b"{}")
        } else if is_json {
            bytes
        } else {
            return Err(ApiError::request(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Corps JSON attendu (Content-Type: application/json)",
            ));
        };
        let value: T = serde_json::from_slice(&bytes)
            .map_err(|error| ApiError::invalid(vec![format!("(corps) : {error}")]))?;
        validate(value).map(Self)
    }
}

/// Query string, validated.
pub struct Query<T>(pub T);

impl<S, T> FromRequestParts<S> for Query<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Query(value) = axum::extract::Query::<T>::try_from_uri(&parts.uri)
            .map_err(|error| {
                ApiError::invalid(vec![format!("(paramètres) : {}", error.body_text())])
            })?;
        validate(value).map(Self)
    }
}

/// The `:id` route parameter, a UUID.
pub struct PathId(pub Uuid);

impl<S: Send + Sync> FromRequestParts<S> for PathId {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(raw) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::invalid(vec!["id : identifiant invalide".into()]))?;
        Uuid::parse_str(&raw)
            .map(Self)
            .map_err(|_| ApiError::invalid(vec!["id : identifiant invalide".into()]))
    }
}
