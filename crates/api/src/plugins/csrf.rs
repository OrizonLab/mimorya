//! CSRF protection: state-changing requests must carry a custom header (not
//! settable cross-site without CORS, which is disabled) and, when present, an
//! `Origin` matching the web application. Three are posted from elsewhere,
//! without it: a SAML provider's response, bound instead by the sealed
//! attempt in its own cookie and the signed response (ADR-068); a delivery
//! to an inbound hook, from another system, which no session authorizes:
//! its token alone does (ADR-109); and an event Slack sends a company's
//! gateway, which its signature alone proves (ADR-125). The latter two's
//! paths are matched whole, their identifier in canonical form.

use axum::extract::{Request, State};
use axum::http::Method;
use axum::http::header::ORIGIN;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use mimorya_core::DomainError;

use crate::app::AppState;
use crate::http::ApiError;
use crate::routes::{HOOK_DELIVERY_PREFIX, SAML_ACS_ROUTE, SLACK_EVENTS_PREFIX};
use uuid::Uuid;

pub const REQUEST_HEADER: &str = "x-mimorya-request";

pub async fn csrf_protection(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) || path == SAML_ACS_ROUTE
        || (*request.method() == Method::POST && (is_hook_delivery(path) || is_gateway_event(path)))
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

/// Whether `path` is a delivery to an inbound hook: the prefix, then a
/// hook's identifier as it is written, lowercase and hyphenated, and nothing
/// else.
fn is_hook_delivery(path: &str) -> bool {
    names_one(path, HOOK_DELIVERY_PREFIX)
}

/// Whether `path` is where Slack sends a gateway's events: the prefix, then
/// a gateway's identifier as it is written, and nothing else.
fn is_gateway_event(path: &str) -> bool {
    names_one(path, SLACK_EVENTS_PREFIX)
}

/// Whether `path` is `prefix` followed by an identifier in canonical form
/// (lowercase, hyphenated) and nothing else.
fn names_one(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .and_then(|id| Uuid::parse_str(id).ok().map(|uuid| (id, uuid)))
        .is_some_and(|(id, uuid)| uuid.hyphenated().to_string() == id)
}

#[cfg(test)]
mod tests {
    use super::{is_gateway_event, is_hook_delivery};

    #[test]
    fn only_a_hook_s_own_path_goes_without_the_header() {
        let id = "0b9c4c1e-7a4f-4f7e-9d1c-2a3b4c5d6e7f";
        assert!(is_hook_delivery(&format!("/hooks/{id}")));
        for path in [
            "/hooks/".to_owned(),
            format!("/hooks/{id}/"),
            format!("/hooks/{}", id.to_uppercase()),
            format!("/hooks/{}", id.replace('-', "")),
            format!("/hooks/{{{id}}}"),
            format!("/hooks/{id}/token"),
            format!("/hooks//{id}"),
            format!("/inbound-hooks/{id}"),
            format!("/hooks/urn:uuid:{id}"),
        ] {
            assert!(!is_hook_delivery(&path), "{path}");
        }
    }

    #[test]
    fn only_a_gateway_s_own_path_goes_without_the_header() {
        let id = "0b9c4c1e-7a4f-4f7e-9d1c-2a3b4c5d6e7f";
        assert!(is_gateway_event(&format!("/gateways/slack/{id}")));
        for path in [
            "/gateways/slack/".to_owned(),
            format!("/gateways/slack/{id}/"),
            format!("/gateways/slack/{}", id.to_uppercase()),
            format!("/gateways/slack/{}", id.replace('-', "")),
            format!("/gateways/slack/{id}/link"),
            format!("/gateways/slack//{id}"),
            format!("/gateways/teams/{id}"),
            format!("/gateways/{id}"),
            format!("/admin/gateways/{id}"),
            format!("/account/gateways/{id}"),
            "/account/gateways/link".to_owned(),
            "/account/gateways/link/preview".to_owned(),
        ] {
            assert!(!is_gateway_event(&path), "{path}");
        }
        // Neither reaches the other's.
        assert!(!is_hook_delivery(&format!("/gateways/slack/{id}")));
        assert!(!is_gateway_event(&format!("/hooks/{id}")));
    }
}
