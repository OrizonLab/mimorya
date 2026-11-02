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

#[cfg(test)]
mod tests {
    use rmcp::model::{
        DiscoverRequest, DiscoverRequestParams, ErrorCode, NumberOrString, PingRequest, RequestId,
    };

    use super::*;

    const ID: RequestId = NumberOrString::Number(3);

    fn discover() -> ClientJsonRpcMessage {
        let request = DiscoverRequest::new(DiscoverRequestParams::default());
        ClientJsonRpcMessage::request(ClientRequest::DiscoverRequest(request), ID)
    }

    /// The error a refusal of the discovery is answered with, if any.
    fn answered(
        message: &ClientJsonRpcMessage,
        attached: bool,
        status: StatusCode,
        body: &str,
    ) -> Option<ErrorData> {
        match discover_refused(message, attached, status, body)? {
            JsonRpcMessage::Error(error) => {
                assert_eq!(error.id, Some(ID), "matched to the discovery");
                Some(error.error)
            }
            answer => panic!("an error: {answer:?}"),
        }
    }

    #[test]
    fn a_discovery_refused_by_a_client_error_is_answered_as_its_error() {
        let error = answered(&discover(), false, StatusCode::NOT_FOUND, "absent").unwrap();
        assert_eq!(error.code, ErrorCode::INVALID_REQUEST);
        assert!(error.message.contains("HTTP 404"), "{}", error.message);
        // The server's own error, if it gave one.
        let own =
            r#"{"jsonrpc":"2.0","id":"server-error","error":{"code":-32601,"message":"inconnue"}}"#;
        let error = answered(&discover(), false, StatusCode::BAD_REQUEST, own).unwrap();
        assert_eq!(error.message, "inconnue");
    }

    #[test]
    fn nothing_else_is_answered_so() {
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            assert!(
                answered(&discover(), false, status, "").is_none(),
                "{status}"
            );
        }
        // Within a session, or another request.
        assert!(answered(&discover(), true, StatusCode::NOT_FOUND, "").is_none());
        let ping = ClientRequest::PingRequest(PingRequest::default());
        let ping = ClientJsonRpcMessage::request(ping, ID);
        assert!(answered(&ping, false, StatusCode::NOT_FOUND, "").is_none());
    }

    #[test]
    fn only_a_whole_json_rpc_error_is_taken_as_such() {
        let error = r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32000,"message":"non"}}"#;
        assert!(json_rpc_error(error).is_some());
        // Cut, as `refused` says it.
        assert!(json_rpc_error(&format!("{}…", &error[..40])).is_none());
        let result = r#"{"jsonrpc":"2.0","id":3,"result":{}}"#;
        assert!(json_rpc_error(result).is_none());
    }
}
