//! Answers that never end, from a test MCP server (ADR-161): JSON, an event
//! of a stream, or an error, each a valid beginning followed by text sent
//! for as long as the client reads it. A client that read them whole would
//! wait until its delay, holding all it read. And a session's stream of one
//! event, of a given size.

use std::convert::Infallible;

use axum::Router;
use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::get;
use bytes::Bytes;
use futures::stream::{self, StreamExt};
use serde_json::Value;

use super::Quirk;

/// What the server sends after the beginning, again and again.
static FILLER: [u8; 64 * 1024] = [b'x'; 64 * 1024];

/// The answer `quirk` gives to the message `body` (JSON-RPC), if it is one
/// of those it never ends.
pub(super) fn endless(quirk: Quirk, body: &[u8]) -> Option<Response> {
    let message: Value = serde_json::from_slice(body).ok()?;
    let method = message["method"].as_str()?;
    let id = &message["id"];
    let result_start =
        format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{{"content":[{{"type":"text","text":""#);
    let (status, content_type, start) = match (quirk, method) {
        (Quirk::EndlessJsonOnCall, "tools/call") => {
            (StatusCode::OK, "application/json", result_start)
        }
        (Quirk::EndlessEventOnCall, "tools/call") => (
            StatusCode::OK,
            "text/event-stream",
            format!("data: {result_start}"),
        ),
        (Quirk::EndlessErrorOnCall, "tools/call") => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "text/plain",
            "Erreur interne : ".to_string(),
        ),
        (Quirk::EndlessJsonOnInitialize, "initialize") => {
            (StatusCode::OK, "application/json", started(id))
        }
        (Quirk::EndlessEventOnInitialize, "initialize") => (
            StatusCode::OK,
            "text/event-stream",
            format!("data: {}", started(id)),
        ),
        (Quirk::EndlessJsonOnList, "tools/list") => {
            let start = format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{{"tools":[{{"name":""#);
            (StatusCode::OK, "application/json", start)
        }
        _ => return None,
    };
    let filler = stream::repeat_with(|| Ok::<_, Infallible>(Bytes::from_static(&FILLER)));
    let body = stream::once(async move { Ok(Bytes::from(start)) }).chain(filler);
    let response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from_stream(body))
        .expect("response");
    Some(response)
}

/// The beginning of an answer to `initialize`.
fn started(id: &Value) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{{"serverInfo":{{"name":""#)
}

/// Serves, at `http://127.0.0.1:<port>/mcp`, an event stream of one event
/// holding `size` bytes of data, as a session's stream (`GET`); returns that
/// address.
pub async fn serve_one_event(size: usize) -> String {
    let event = format!("data: {}\n\n", "x".repeat(size));
    let answer = move || {
        let event = event.clone();
        async move {
            Response::builder()
                .header(header::CONTENT_TYPE, "text/event-stream")
                .body(Body::from(event))
                .expect("response")
        }
    };
    super::serve(Router::new().route("/mcp", get(answer)), 0).await
}
