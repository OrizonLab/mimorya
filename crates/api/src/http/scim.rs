//! SCIM over HTTP (RFC 7644, ADR-183): a refusal written as SCIM writes it,
//! a response in `application/scim+json`, and a body read whether the
//! provider sends it as `application/scim+json` or `application/json`. A
//! body is judged only once the token is: what is wrong with it is kept for
//! the service, which refuses it after the token (an absent token is always
//! refused as such, and a refused request still notes the token's use).

use axum::Json;
use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::http::header::{CONTENT_TYPE, LOCATION, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;

use crate::contract::{SCIM_ERROR_SCHEMA, ScimErrorBody};

const SCIM_JSON: &str = "application/scim+json";

/// A refusal, said in French for the provider's logs.
#[derive(Debug)]
pub struct ScimError {
    status: StatusCode,
    scim_type: Option<&'static str>,
    detail: String,
}

impl ScimError {
    fn new(status: StatusCode, scim_type: Option<&'static str>, detail: impl Into<String>) -> Self {
        Self {
            status,
            scim_type,
            detail: detail.into(),
        }
    }

    /// No token, an unknown one, a removed one or a suspended company's.
    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            None,
            "Jeton SCIM absent, inconnu ou retiré",
        )
    }

    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, None, "Compte introuvable")
    }

    pub fn invalid_filter(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, Some("invalidFilter"), detail)
    }

    pub fn invalid_value(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, Some("invalidValue"), detail)
    }

    pub fn invalid_syntax(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, Some("invalidSyntax"), detail)
    }

    pub fn uniqueness(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, Some("uniqueness"), detail)
    }

    pub fn mutability(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, Some("mutability"), detail)
    }

    /// A path under the SCIM prefix that serves nothing.
    pub fn no_such_path() -> Self {
        Self::new(StatusCode::NOT_FOUND, None, "Ressource SCIM introuvable")
    }

    /// A method the path does not serve.
    pub fn method_not_allowed() -> Self {
        Self::new(
            StatusCode::METHOD_NOT_ALLOWED,
            None,
            "Méthode non servie à cette adresse",
        )
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }
}

/// What went wrong inside is logged, never said: the provider is told the
/// request failed.
impl From<anyhow::Error> for ScimError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(error = %format!("{error:#}"), "SCIM request failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            "Erreur interne : réessayez plus tard",
        )
    }
}

impl From<sqlx::Error> for ScimError {
    fn from(error: sqlx::Error) -> Self {
        anyhow::Error::from(error).into()
    }
}

impl IntoResponse for ScimError {
    fn into_response(self) -> Response {
        let body = ScimErrorBody {
            schemas: vec![SCIM_ERROR_SCHEMA.into()],
            status: self.status.as_u16().to_string(),
            scim_type: self.scim_type.map(str::to_string),
            detail: self.detail,
        };
        let mut response = ScimJson::new(self.status, body).into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer realm=\"scim\""),
            );
        }
        response
    }
}

pub type ScimResult<T> = Result<T, ScimError>;

/// A response in `application/scim+json`, with where the resource lives
/// when one is made.
pub struct ScimJson<T> {
    status: StatusCode,
    body: T,
    location: Option<String>,
}

impl<T> ScimJson<T> {
    pub fn new(status: StatusCode, body: T) -> Self {
        Self {
            status,
            body,
            location: None,
        }
    }

    pub fn ok(body: T) -> Self {
        Self::new(StatusCode::OK, body)
    }

    /// A resource made, living at `location`.
    pub fn created(body: T, location: String) -> Self {
        Self {
            status: StatusCode::CREATED,
            body,
            location: Some(location),
        }
    }
}

impl<T: Serialize> IntoResponse for ScimJson<T> {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.body)).into_response();
        let headers = response.headers_mut();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(SCIM_JSON));
        if let Some(location) = self.location.and_then(|l| HeaderValue::from_str(&l).ok()) {
            headers.insert(LOCATION, location);
        }
        response
    }
}

/// A SCIM body: JSON sent as `application/scim+json` or `application/json`,
/// or why it cannot be read, refused only after the token.
pub struct ScimBody(pub ScimResult<Value>);

impl<S> FromRequest<S> for ScimBody
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(read_body(request, state).await))
    }
}

async fn read_body<S: Send + Sync>(request: Request, state: &S) -> ScimResult<Value> {
    let json = request
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(str::trim)
        .is_some_and(|v| {
            v.eq_ignore_ascii_case(SCIM_JSON) || v.eq_ignore_ascii_case("application/json")
        });
    if !json {
        return Err(ScimError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            None,
            "Corps attendu en application/scim+json",
        ));
    }
    let bytes = Bytes::from_request(request, state).await.map_err(|_| {
        ScimError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            None,
            "Corps de requête trop volumineux",
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|_| ScimError::invalid_syntax("Corps JSON illisible"))
}
