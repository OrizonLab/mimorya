//! The HTTP client of an MCP server's transport (ADR-161). The MCP
//! library's own client read whole the JSON answers and the refusals of the
//! messages it posted: a server could make the process hold as much as it
//! sent, within the start or call delay. This one posts the messages as the
//! library does, but reads their answers within bounds (`body`, `events`,
//! `refused`), noting each answer it cut: the library does not say why a
//! start failed, the connector does. The library's own client still opens
//! the session's event stream, each event bounded at `MAX_RESPONSE_BYTES`,
//! and ends the session, neither reading a body whole.

mod body;
mod events;
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

pub use body::{Cuts, MAX_RESPONSE_BYTES, TOO_LONG};
pub use post::HttpError;

/// The client of `outbound::http_client`, reading answers within bounds.
#[derive(Clone)]
pub struct BoundedHttpClient {
    client: reqwest_mcp::Client,
    cuts: Cuts,
}

impl BoundedHttpClient {
    pub fn new(client: reqwest_mcp::Client) -> Self {
        Self {
            client,
            cuts: Cuts::default(),
        }
    }

    /// The answers this client, and every clone of it, cut.
    pub fn cuts(&self) -> Cuts {
        self.cuts.clone()
    }
}

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
            &self.client,
            &self.cuts,
            uri,
            message,
            session_id,
            auth_header,
            custom_headers,
        )
        .await
    }

    /// Its bound is `MAX_RESPONSE_BYTES` in all, whatever the transport's
    /// per event.
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
            &self.client,
            &self.cuts,
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
        let client = &self.client;
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
        let client = &self.client;
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
        let client = &self.client;
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
