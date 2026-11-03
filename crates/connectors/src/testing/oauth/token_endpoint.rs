//! The token endpoint of the simulated authorization server, as strict as
//! the platform must expect a real one to be: the client proves itself one
//! way only, among those the server lists (RFC 6749 §2.3), a refused Basic
//! proof answered 401 (§5.2); the code is spent at its first use; the PKCE
//! verifier must be well formed and match (RFC 7636); the resource must be
//! the one authorized (RFC 8707, `invalid_target`); a refresh token rotates,
//! and one already rotated, used again, revokes the whole grant.

use std::collections::HashMap;

use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::authorization_server::{
    AuthRequest, CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, Inner, Refresh, issue_access,
    token_value,
};

pub(super) async fn token(
    State(server): State<FakeAuthorizationServer>,
    headers: HeaderMap,
    Form(params): Form<HashMap<String, String>>,
) -> Response {
    let mut inner = server.inner.lock().expect("server");
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .map(|v| {
            STANDARD
                .decode(v)
                .ok()
                .and_then(|v| String::from_utf8(v).ok())
                .unwrap_or_default()
        });
    inner.requests.push(AuthRequest {
        endpoint: "token",
        params: params.clone(),
        basic: basic.clone(),
    });
    if let Some(refusal) = client_refusal(&inner.settings.auth_methods, basic.as_deref(), &params) {
        return refusal;
    }
    let field = |name: &str| params.get(name).cloned().unwrap_or_default();
    let resource = params.get("resource").cloned();
    match field("grant_type").as_str() {
        "authorization_code" => {
            // Spent at its first use, whatever comes of it.
            let Some(code) = inner.codes.remove(&field("code")) else {
                return refused("invalid_grant");
            };
            let verifier = field("code_verifier");
            let verified = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
            if code.redirect_uri != field("redirect_uri")
                || !well_formed(&verifier)
                || verified != code.challenge
            {
                return refused("invalid_grant");
            }
            if code.resource != resource {
                return refused("invalid_target");
            }
            inner.next += 1;
            let grant = inner.next;
            granted(&mut inner, grant, code.resource, code.scope)
        }
        "refresh_token" => {
            let given = field("refresh_token");
            let Some(refresh) = inner.refresh.get(&given) else {
                return refused("invalid_grant");
            };
            let (grant, current) = (refresh.grant, refresh.current);
            let (granted_for, scope) = (refresh.resource.clone(), refresh.scope.clone());
            if !current {
                // A rotated token used again: the whole grant is revoked.
                inner.revoked.push(grant);
                return refused("invalid_grant");
            }
            if inner.revoked.contains(&grant) {
                return refused("invalid_grant");
            }
            if granted_for != resource {
                return refused("invalid_target");
            }
            if let Some(refresh) = inner.refresh.get_mut(&given) {
                refresh.current = false;
            }
            granted(&mut inner, grant, granted_for, scope)
        }
        _ => refused("unsupported_grant_type"),
    }
}

/// The refusal of a client that did not prove itself one way only, a way
/// the server lists, with its own id and secret.
fn client_refusal(
    methods: &[String],
    basic: Option<&str>,
    params: &HashMap<String, String>,
) -> Option<Response> {
    let (method, id, secret) = match (basic, params.get("client_secret")) {
        (Some(_), Some(_)) => return Some(refused("invalid_client")),
        (Some(pair), None) => match pair.split_once(':') {
            Some((id, secret)) => ("client_secret_basic", id, Some(secret)),
            None => return Some(refused_basic()),
        },
        (None, Some(secret)) => (
            "client_secret_post",
            params.get("client_id").map_or("", String::as_str),
            Some(secret.as_str()),
        ),
        (None, None) => (
            "none",
            params.get("client_id").map_or("", String::as_str),
            None,
        ),
    };
    let proven = methods.iter().any(|m| m == method)
        && id == CLIENT_ID
        && secret.is_none_or(|secret| secret == CLIENT_SECRET);
    match (proven, basic.is_some()) {
        (true, _) => None,
        (false, true) => Some(refused_basic()),
        (false, false) => Some(refused("invalid_client")),
    }
}

/// A PKCE verifier as RFC 7636 §4.1 writes one: 43 to 128 unreserved
/// characters.
fn well_formed(verifier: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
}

fn granted(inner: &mut Inner, grant: u64, resource: Option<String>, scope: String) -> Response {
    let access = issue_access(inner, grant, resource.clone());
    let mut body = json!({ "access_token": access, "token_type": "Bearer", "scope": scope });
    if let Some(lifetime) = inner.settings.lifetime {
        body["expires_in"] = Value::from(lifetime);
    }
    if inner.settings.refresh {
        let refresh = token_value(inner, "renouvellement");
        inner.refresh.insert(
            refresh.clone(),
            Refresh {
                grant,
                current: true,
                resource,
                scope,
            },
        );
        body["refresh_token"] = Value::from(refresh);
    }
    axum::Json(body).into_response()
}

fn refused(error: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        axum::Json(json!({ "error": error })),
    )
        .into_response()
}

/// A refused Basic proof (RFC 6749 §5.2): 401, with the scheme to use.
fn refused_basic() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"jetons\"")],
        axum::Json(json!({ "error": "invalid_client" })),
    )
        .into_response()
}
