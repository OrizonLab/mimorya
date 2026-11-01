//! An MCP server's refusal of a message (any status but a success), read up
//! to `MAX_ERROR_BYTES` (ADR-161) and told as the MCP library's own client
//! tells it: the server's JSON-RPC error if it gave one, a rejected
//! `server/discover` as an error the protocol's lifecycle can classify,
//! otherwise the status and what was read of the body.

use std::borrow::Cow;

use reqwest_mcp::{Response, StatusCode};
use rmcp::model::ServerJsonRpcMessage;
use rmcp::model::{ClientJsonRpcMessage, ClientRequest, ErrorData, JsonRpcMessage};
use rmcp::transport::streamable_http_client::{StreamableHttpError, StreamableHttpPostResponse};

use super::body::{MAX_ERROR_BYTES, Read, read_within};
use super::post::HttpError;

/// The refusal `response` of `message`, as the transport takes it.
pub async fn refused(
    response: Response,
    message: &ClientJsonRpcMessage,
    session_was_attached: bool,
    is_json: bool,
    session_id: Option<String>,
) -> Result<StreamableHttpPostResponse, HttpError> {
    let status = response.status();
    let body = match read_within(response, MAX_ERROR_BYTES).await {
        Ok(Read::Whole(body)) => String::from_utf8_lossy(&body).into_owned(),
        // What was read, said to be cut: never a whole JSON-RPC error.
        Ok(Read::Cut(body)) => format!("{}…", String::from_utf8_lossy(&body)),
        Err(_) => "<failed to read response body>".to_owned(),
    };
    // Before the JSON-RPC error below, which would carry an identifier the
    // lifecycle cannot match to its discovery.
    if let Some(answer) = discover_refused(message, session_was_attached, status, &body) {
        return Ok(StreamableHttpPostResponse::Json(answer, None));
    }
    if is_json && let Some(answer) = json_rpc_error(&body) {
        return Ok(StreamableHttpPostResponse::Json(answer, session_id));
    }
    Err(StreamableHttpError::UnexpectedServerResponse(Cow::Owned(
        format!("HTTP {status}: {body}"),
    )))
}

/// `body` if it is a JSON-RPC error.
fn json_rpc_error(body: &str) -> Option<ServerJsonRpcMessage> {
    match serde_json::from_str::<ServerJsonRpcMessage>(body) {
        Ok(answer @ JsonRpcMessage::Error(_)) => Some(answer),
        _ => None,
    }
}

/// A server that does not know `server/discover` (one before it, or a proxy
/// in front of it) refuses it with a client error: answered as an error of
/// that discovery, the server's own if it gave one, for the lifecycle to try
/// `initialize` instead. Not a refusal of the token, nor a server error.
fn discover_refused(
    message: &ClientJsonRpcMessage,
    session_was_attached: bool,
    status: StatusCode,
    body: &str,
) -> Option<ServerJsonRpcMessage> {
    if session_was_attached
        || !status.is_client_error()
        || matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
    {
        return None;
    }
    let ClientJsonRpcMessage::Request(request) = message else {
        return None;
    };
    if !matches!(request.request, ClientRequest::DiscoverRequest(_)) {
        return None;
    }
    let error = match serde_json::from_str::<ServerJsonRpcMessage>(body) {
        Ok(ServerJsonRpcMessage::Error(error)) => error.error,
        _ => ErrorData::invalid_request(
            format!("server/discover rejected with HTTP {status}: {body}"),
            None,
        ),
    };
    Some(ServerJsonRpcMessage::error(error, Some(request.id.clone())))
}
