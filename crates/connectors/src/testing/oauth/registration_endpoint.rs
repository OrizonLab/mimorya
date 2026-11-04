//! The registration endpoint of the simulated authorization server (RFC
//! 7591): a client asked for absolute return addresses, the authorization
//! code grant and a way to prove itself the server lists is registered,
//! with a secret unless it asked to be public, and accepted from then on by
//! the other endpoints the way it registered. The tests can make it ask for
//! an initial access token, or give secrets that expire.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::authorization_server::{AuthRequest, Client, FakeAuthorizationServer, token_value};
use super::token_endpoint::refused;

pub(super) async fn register(
    State(server): State<FakeAuthorizationServer>,
    headers: HeaderMap,
    Json(asked): Json<Value>,
) -> Response {
    let mut inner = server.inner.lock().expect("server");
    let words = |name: &str| -> Vec<String> {
        asked[name]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    let text = |name: &str| asked[name].as_str().unwrap_or_default().to_string();
    let (redirect_uris, grant_types) = (words("redirect_uris"), words("grant_types"));
    let method = match text("token_endpoint_auth_method") {
        method if method.is_empty() => "client_secret_basic".to_string(),
        method => method,
    };
    let params = HashMap::from([
        ("client_name".to_string(), text("client_name")),
        ("redirect_uris".to_string(), redirect_uris.join(" ")),
        ("grant_types".to_string(), grant_types.join(" ")),
        (
            "response_types".to_string(),
            words("response_types").join(" "),
        ),
        ("token_endpoint_auth_method".to_string(), method.clone()),
        ("scope".to_string(), text("scope")),
    ]);
    inner.requests.push(AuthRequest {
        endpoint: "register",
        params,
        basic: None,
    });
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("Bearer "));
    if inner.settings.registration_token && !bearer {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid_token" })),
        )
            .into_response();
    }
    if redirect_uris.is_empty()
        || redirect_uris
            .iter()
            .any(|uri| url::Url::parse(uri).is_err())
    {
        return refused("invalid_redirect_uri");
    }
    if !grant_types.iter().any(|g| g == "authorization_code")
        || !inner.settings.auth_methods.contains(&method)
    {
        return refused("invalid_client_metadata");
    }
    let client_id = token_value(&mut inner, "client");
    let secret = (method != "none").then(|| token_value(&mut inner, "secret-client"));
    let expires_at = inner.settings.secret_lifetime.map_or(0, |lifetime| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |now| now.as_secs())
            + lifetime
    });
    inner.clients.insert(
        client_id.clone(),
        Client {
            secret: secret.clone(),
            method: method.clone(),
            redirect_uris: redirect_uris.clone(),
        },
    );
    let mut answer = json!({
        "client_id": client_id,
        "client_id_issued_at": 1_700_000_000,
        "token_endpoint_auth_method": method,
        "redirect_uris": redirect_uris,
        "grant_types": grant_types,
        "client_name": text("client_name"),
    });
    if let Some(secret) = secret {
        answer["client_secret"] = Value::from(secret);
        answer["client_secret_expires_at"] = Value::from(expires_at);
    }
    (StatusCode::CREATED, Json(answer)).into_response()
}
