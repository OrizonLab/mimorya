//! Where another system posts to a person's inbound hook (ADR-109), before
//! any session: its token, sent as `Authorization: Bearer <token>`, is its
//! only proof, so the request header the browser's requests carry is not
//! asked of it (`plugins/csrf.rs`). An unknown hook, a wrong or missing token
//! and a paused hook answer alike (404). At most `MAX_BODY_BYTES` of text are
//! taken (413), as UTF-8 (415). Accepted, it answers 202, naming nothing.

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{AUTHORIZATION, CONTENT_LENGTH, RETRY_AFTER};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use chrono::Utc;
use uuid::Uuid;

use crate::app::AppState;
use crate::http::{ApiError, ApiResult};
use crate::services::inbound_hooks::{MAX_BODY_BYTES, Refusal, deliver, unknown};

/// Where a system posts: the only route a request without the browser's
/// header reaches beside the SAML reception (`plugins/csrf.rs`).
pub const DELIVERY_PREFIX: &str = "/hooks/";

const TOO_LARGE: &str = "Données reçues trop volumineuses : 16 Kio au plus.";
const NOT_TEXT: &str = "Données reçues illisibles : du texte UTF-8 est attendu.";
const LIMITED: &str = "Ce déclencheur a reçu trop d’envois dans l’heure : réessayez plus tard.";

pub fn routes() -> Router<AppState> {
    Router::new().route("/hooks/{id}", post(receive))
}

async fn receive(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<Response> {
    let (Ok(id), Some(token)) = (Uuid::parse_str(&raw), bearer(&headers)) else {
        return Err(unknown().into());
    };
    let announced = headers
        .get(CONTENT_LENGTH)
        .and_then(|length| length.to_str().ok())
        .and_then(|length| length.parse::<u64>().ok());
    if announced.is_some_and(|length| length > MAX_BODY_BYTES as u64) {
        return Err(ApiError::request(StatusCode::PAYLOAD_TOO_LARGE, TOO_LARGE));
    }
    let bytes = axum::body::to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| ApiError::request(StatusCode::PAYLOAD_TOO_LARGE, TOO_LARGE))?;
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Err(ApiError::request(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            NOT_TEXT,
        ));
    };
    match deliver(&state, id, token, text, Utc::now()).await? {
        Ok(_) => Ok(StatusCode::ACCEPTED.into_response()),
        Err(Refusal::Unknown) => Err(unknown().into()),
        Err(Refusal::Limited { until }) => {
            let wait = (until - Utc::now()).num_seconds().max(1);
            let mut response = ApiError::Throttled(LIMITED).into_response();
            if let Ok(value) = HeaderValue::from_str(&wait.to_string()) {
                response.headers_mut().insert(RETRY_AFTER, value);
            }
            Ok(response)
        }
        Err(Refusal::Refused(problem)) => Err(mimorya_core::DomainError::conflict(problem).into()),
        Err(Refusal::Failed(problem)) => {
            Err(mimorya_core::DomainError::unavailable(problem).into())
        }
    }
}

/// The token sent as `Authorization: Bearer <token>`, if any.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}
