//! A REST API on a local port, for tests (feature `testing`, ADR-112): it
//! tells what it received (method, path, query, token headers, body), and
//! misbehaves on demand: a token quoted back, cut by the answer's bound or
//! not, an error status, a redirect, a binary body, an answer that never
//! comes.

use std::net::SocketAddr;

use axum::Router;
use axum::body::Bytes;
use axum::extract::Path;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, LOCATION};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use serde_json::{Value, json};

use crate::rest::MAX_RESPONSE_BYTES;

/// The header the tests send a key in rather than `Authorization`.
pub const KEY_HEADER: &str = "X-Api-Key";

/// Serves the test API until the process ends; returns its address,
/// `http://127.0.0.1:<port>/api`.
pub async fn serve_rest_http() -> String {
    let router = Router::new()
        .route("/api/echo/{*rest}", any(echo))
        .route("/api/token", get(token))
        .route("/api/cut-token", get(cut_token))
        .route("/api/status/{code}", get(status))
        .route("/api/redirect", get(redirect))
        .route("/api/binary", get(binary))
        .route("/api/plain", get(plain))
        .route("/api/slow", get(slow));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("local port");
    let address: SocketAddr = listener.local_addr().expect("local address");
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("test REST API");
    });
    format!("http://{address}/api")
}

fn header(headers: &HeaderMap, name: &str) -> Value {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map_or(Value::Null, |value| json!(value))
}

/// The request as received.
async fn echo(method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Response {
    let received = json!({
        "method": method.as_str(),
        "path": uri.path(),
        "query": uri.query(),
        "authorization": header(&headers, AUTHORIZATION.as_str()),
        "key": header(&headers, KEY_HEADER),
        "contentType": header(&headers, CONTENT_TYPE.as_str()),
        "body": serde_json::from_slice::<Value>(&body).ok(),
    });
    axum::Json(received).into_response()
}

/// The token received, quoted back in an error, as servers sometimes do.
async fn token(headers: HeaderMap) -> Response {
    let received = headers
        .get(KEY_HEADER)
        .or_else(|| headers.get(AUTHORIZATION))
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = json!({ "erreur": format!("jeton refusé : {received}") });
    (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
}

/// A text longer than the bound: the token, then `x` up to `?at=` bytes,
/// where the token is written again, then `y`.
async fn cut_token(uri: Uri, headers: HeaderMap) -> Response {
    let received = headers
        .get(KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let at: usize = uri
        .query()
        .and_then(|query| query.strip_prefix("at="))
        .and_then(|at| at.parse().ok())
        .unwrap_or(MAX_RESPONSE_BYTES);
    let mut text = received.to_string();
    text.push_str(&"x".repeat(at.saturating_sub(received.len())));
    text.push_str(received);
    text.push_str(&"y".repeat(1000));
    ([(CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response()
}

async fn status(Path(code): Path<u16>) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        status,
        axum::Json(json!({ "erreur": "ressource introuvable" })),
    )
        .into_response()
}

async fn redirect(uri: Uri) -> Response {
    let target = uri.path().replace("/redirect", "/echo/redirected");
    (StatusCode::FOUND, [(LOCATION, target)]).into_response()
}

async fn binary() -> Response {
    (
        [(CONTENT_TYPE, "image/png")],
        vec![0x89u8, b'P', b'N', b'G', 0, 1, 2],
    )
        .into_response()
}

async fn plain() -> Response {
    ([(CONTENT_TYPE, "text/plain")], "Bonjour.").into_response()
}

async fn slow() -> Response {
    std::future::pending::<()>().await;
    StatusCode::OK.into_response()
}
