//! A message posted to an MCP server, and its answer read within bounds
//! (ADR-161): as the MCP library's own client posts it (rmcp 3.5,
//! `transport/common/reqwest/streamable_http_client.rs`), but for the
//! bodies that client read whole, read here as `body` allows.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use reqwest_mcp::header::{ACCEPT, CONTENT_TYPE, HeaderName, HeaderValue, WWW_AUTHENTICATE};
use reqwest_mcp::{Client, Response, StatusCode};
use rmcp::model::{ClientJsonRpcMessage, ServerJsonRpcMessage};
use rmcp::transport::common::http_header::{
    EVENT_STREAM_MIME_TYPE, HEADER_LAST_EVENT_ID, HEADER_MCP_PROTOCOL_VERSION, HEADER_SESSION_ID,
    JSON_MIME_TYPE,
};
use rmcp::transport::streamable_http_client::{
    AuthRequiredError, InsufficientScopeError, StreamableHttpError, StreamableHttpPostResponse,
};

use super::body::{MAX_RESPONSE_BYTES, Read, bounded_events, read_within, too_long};
use super::refused::refused;

pub type HttpError = StreamableHttpError<reqwest_mcp::Error>;

/// Posts `message`, then reads the server's answer within bounds.
pub async fn post(
    client: &Client,
    uri: Arc<str>,
    message: ClientJsonRpcMessage,
    session_id: Option<Arc<str>>,
    auth_token: Option<String>,
    custom_headers: HashMap<HeaderName, HeaderValue>,
) -> Result<StreamableHttpPostResponse, HttpError> {
    let accepted = [EVENT_STREAM_MIME_TYPE, JSON_MIME_TYPE].join(", ");
    let mut request = client.post(uri.as_ref()).header(ACCEPT, accepted);
    if let Some(token) = auth_token {
        request = request.bearer_auth(token);
    }
    for (name, value) in custom_headers {
        if reserved(&name) {
            return Err(StreamableHttpError::ReservedHeaderConflict(
                name.to_string(),
            ));
        }
        request = request.header(name, value);
    }
    let session_was_attached = session_id.is_some();
    if let Some(session_id) = session_id {
        request = request.header(HEADER_SESSION_ID, session_id.as_ref());
    }
    let response = request.json(&message).send().await?;
    denied(&response)?;
    let status = response.status();
    if matches!(status, StatusCode::ACCEPTED | StatusCode::NO_CONTENT) {
        return Ok(StreamableHttpPostResponse::Accepted);
    }
    if status == StatusCode::NOT_FOUND && session_was_attached {
        return Err(StreamableHttpError::SessionExpired);
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .map(|value| String::from_utf8_lossy(value.as_bytes()).to_string());
    let new_session_id = response
        .headers()
        .get(HEADER_SESSION_ID)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    // Some servers answer what awaits no answer with an empty 200.
    if status.is_success() && response.content_length() == Some(0) && awaits_none(&message) {
        return Ok(StreamableHttpPostResponse::Accepted);
    }
    if !status.is_success() {
        let is_json = content_type
            .as_deref()
            .is_some_and(|ct| is(ct, JSON_MIME_TYPE));
        let attached = session_was_attached;
        return refused(response, &message, attached, is_json, new_session_id).await;
    }
    match content_type.as_deref() {
        Some(ct) if is(ct, EVENT_STREAM_MIME_TYPE) => {
            let events = bounded_events(response, too_long(&message));
            Ok(StreamableHttpPostResponse::Sse(events, new_session_id))
        }
        Some(ct) if is(ct, JSON_MIME_TYPE) => json(response, &message, new_session_id).await,
        _ => Err(StreamableHttpError::UnexpectedContentType(content_type)),
    }
}

/// A JSON answer, read within `MAX_RESPONSE_BYTES`.
async fn json(
    response: Response,
    message: &ClientJsonRpcMessage,
    session_id: Option<String>,
) -> Result<StreamableHttpPostResponse, HttpError> {
    let body = match read_within(response, MAX_RESPONSE_BYTES).await? {
        Read::Whole(body) => body,
        Read::Cut(_) => {
            return Ok(match too_long(message) {
                Some(answer) => StreamableHttpPostResponse::Json(answer, session_id),
                None => StreamableHttpPostResponse::Accepted,
            });
        }
    };
    match serde_json::from_slice::<ServerJsonRpcMessage>(&body) {
        Ok(answer) => Ok(StreamableHttpPostResponse::Json(answer, session_id)),
        // What awaits no answer is accepted whatever its body; a request
        // needs one, lest it wait for an event that never comes.
        Err(_) if awaits_none(message) => Ok(StreamableHttpPostResponse::Accepted),
        Err(error) => Err(StreamableHttpError::UnexpectedServerResponse(Cow::Owned(
            format!(
                "could not parse JSON response as ServerJsonRpcMessage: {error}: {}",
                preview(&body)
            ),
        ))),
    }
}

/// A refusal of the token: 401, or 403 for want of a scope (the platform
/// asks for none: the scope is not read).
fn denied(response: &Response) -> Result<(), HttpError> {
    let status = response.status();
    if status != StatusCode::UNAUTHORIZED && status != StatusCode::FORBIDDEN {
        return Ok(());
    }
    let Some(header) = response.headers().get(WWW_AUTHENTICATE) else {
        return Ok(());
    };
    let header = header
        .to_str()
        .map_err(|_| {
            StreamableHttpError::UnexpectedServerResponse(Cow::Borrowed(
                "invalid www-authenticate header value",
            ))
        })?
        .to_string();
    Err(if status == StatusCode::UNAUTHORIZED {
        StreamableHttpError::AuthRequired(AuthRequiredError::new(header))
    } else {
        StreamableHttpError::InsufficientScope(InsufficientScopeError::new(header, None))
    })
}

/// Headers the transport alone sets; the protocol version it adds itself.
fn reserved(name: &HeaderName) -> bool {
    let name = name.as_str();
    !name.eq_ignore_ascii_case(HEADER_MCP_PROTOCOL_VERSION)
        && ["accept", HEADER_SESSION_ID, HEADER_LAST_EVENT_ID]
            .iter()
            .any(|reserved| name.eq_ignore_ascii_case(reserved))
}

/// A notification, a reply or an error: what awaits no answer.
fn awaits_none(message: &ClientJsonRpcMessage) -> bool {
    !matches!(message, ClientJsonRpcMessage::Request(_))
}

fn is(content_type: &str, mime: &str) -> bool {
    content_type.as_bytes().starts_with(mime.as_bytes())
}

/// The first 256 bytes of a body that is not JSON-RPC, as text.
fn preview(body: &[u8]) -> String {
    if body.is_empty() {
        return "<empty>".into();
    }
    let text = String::from_utf8_lossy(body);
    let mut end = text.len().min(256);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}
