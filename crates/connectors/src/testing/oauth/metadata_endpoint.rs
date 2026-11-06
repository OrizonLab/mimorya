//! The simulated authorization server's metadata (RFC 8414): its endpoints,
//! what it supports, and whether its return names the issuer; another
//! issuer named at will, the answer kept back at will (`held_metadata`),
//! each request counted.

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::authorization_server::FakeAuthorizationServer;

pub(super) async fn metadata(State(server): State<FakeAuthorizationServer>) -> Response {
    let settings = {
        let mut inner = server.inner.lock().expect("server");
        inner.metadata_asked += 1;
        inner.settings.clone()
    };
    if let Some(held) = settings.held_metadata {
        tokio::time::sleep(held).await;
    }
    let issuer = &server.issuer;
    let named = if settings.wrong_issuer {
        format!("{issuer}/autre")
    } else {
        issuer.clone()
    };
    let revocation = settings.revocation.then(|| format!("{issuer}/revoke"));
    let registration = settings.registration.then(|| format!("{issuer}/register"));
    axum::Json(json!({
        "issuer": named,
        "revocation_endpoint": revocation,
        "registration_endpoint": registration,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "response_types_supported": ["code"],
        "code_challenge_methods_supported": ["S256"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "token_endpoint_auth_methods_supported": settings.auth_methods,
        "scopes_supported": ["files:read", "offline_access"],
        "authorization_response_iss_parameter_supported": settings.iss_on_return,
    }))
    .into_response()
}
