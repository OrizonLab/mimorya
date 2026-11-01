//! The HTTP client of an MCP server's transport (ADR-161). The MCP
//! library's own client read whole the JSON answers and the refusals of the
//! messages it posted: a server could make the process hold as much as it
//! sent, within the start or call delay. This one posts the messages as the
//! library does, but reads their answers within bounds (`body`, `refused`);
//! the library's own client still opens the session's event stream (each
//! event bounded as the transport configures it: `MAX_RESPONSE_BYTES`) and
//! ends the session, neither reading a body whole.

mod body;
mod post;
mod refused;

use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::BoxStream;
use reqwest_mcp::header::{HeaderName, HeaderValue};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::streamable_http_client::{
    SseError, StreamableHttpClient, StreamableHttpPostResponse,
};
use sse_stream::Sse;

use post::HttpError;

pub use body::{MAX_ERROR_BYTES, MAX_RESPONSE_BYTES, TOO_LONG};

/// The client of `outbound::http_client`, reading answers within bounds.
#[derive(Clone)]
pub struct BoundedHttpClient(pub reqwest_mcp::Client);

impl StreamableHttpClient for BoundedHttpClient {
    type Error = reqwest_mcp::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, HttpError> {
        post::post(
            &self.0,
            uri,
            message,
            session_id,
            auth_header,
            custom_headers,
        )
        .await
    }

    /// Its bound is `MAX_RESPONSE_BYTES` in all, whatever the transport's
    /// per event (the same: see `connector`).
    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        _max_sse_event_size: usize,
    ) -> Result<StreamableHttpPostResponse, HttpError> {
        post::post(
            &self.0,
            uri,
            message,
            session_id,
            auth_header,
            custom_headers,
        )
        .await
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), HttpError> {
        let client = &self.0;
        client
            .delete_session(uri, session_id, auth_header, custom_headers)
            .await
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, HttpError> {
        let client = &self.0;
        client
            .get_stream_with_max_sse_event_size(
                uri,
                session_id,
                last_event_id,
                auth_header,
                custom_headers,
                MAX_RESPONSE_BYTES,
            )
            .await
    }

    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, HttpError> {
        let client = &self.0;
        let max = max_sse_event_size.min(MAX_RESPONSE_BYTES);
        client
            .get_stream_with_max_sse_event_size(
                uri,
                session_id,
                last_event_id,
                auth_header,
                custom_headers,
                max,
            )
            .await
    }
}
