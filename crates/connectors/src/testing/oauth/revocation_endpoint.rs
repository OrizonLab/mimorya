//! The revocation endpoint of the simulated authorization server (RFC 7009):
//! the client proves itself as at the token endpoint; a token it issued,
//! access or refresh, revokes its whole grant; one it does not know is
//! answered as revoked all the same (§2.2). Unless the tests turn it mute,
//! or make it one that revokes no access token (§2.2.1).

use std::collections::HashMap;
use std::time::Duration;

use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::authorization_server::{AuthRequest, FakeAuthorizationServer};
use super::token_endpoint::{basic_credentials, client_refusal};

/// How long a mute revocation endpoint keeps the request: longer than the
/// platform waits.
const MUTE: Duration = Duration::from_secs(60);

pub(super) async fn revoke(
    State(server): State<FakeAuthorizationServer>,
    headers: HeaderMap,
    Form(params): Form<HashMap<String, String>>,
) -> Response {
    let mute = {
        let mut inner = server.inner.lock().expect("server");
        let basic = basic_credentials(&headers);
        inner.requests.push(AuthRequest {
            endpoint: "revoke",
            params: params.clone(),
            basic: basic.clone(),
        });
        if let Some(refusal) = client_refusal(&inner, basic.as_deref(), &params) {
            return refusal;
        }
        let token = params.get("token").map_or("", String::as_str);
        if !inner.settings.revokes_access && inner.issued_access(token) {
            let body = json!({ "error": "unsupported_token_type" });
            return (StatusCode::BAD_REQUEST, axum::Json(body)).into_response();
        }
        if !inner.settings.mute_revocation
            && let Some(grant) = inner.grant_of(token)
        {
            inner.revoked.push(grant);
        }
        inner.settings.mute_revocation
    };
    if mute {
        tokio::time::sleep(MUTE).await;
    }
    StatusCode::OK.into_response()
}
