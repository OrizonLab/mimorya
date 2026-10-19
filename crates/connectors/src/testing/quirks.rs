//! How a test MCP server misbehaves: what it declares (names colliding once
//! namespaced, too many tools, a huge schema), or errors quoting the
//! `Authorization` header back (some servers do), at start or on tool calls.

use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData,
    Implementation, JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities,
    ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler};
use serde_json::json;

use crate::mcp::MAX_TOOLS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Quirk {
    #[default]
    None,
    /// Two tools, `get-item` and `get_item`: one identifier once namespaced.
    CollidingTools,
    /// More tools than a connector accepts.
    TooManyTools,
    /// A tool whose input schema is too large.
    HugeSchema,
    /// Every request is refused with an error quoting the token.
    EchoesTokenOnStart,
    /// Tool calls are refused with an error quoting the token.
    EchoesTokenOnCall,
    /// Tools quoting the token in their results: `echo_text` as text,
    /// `echo_structured` as structured content.
    EchoesTokenInResults,
}

/// A server declaring the tools a quirk calls for; each answers "ok".
#[derive(Clone)]
pub struct DeclaredToolsServer {
    tools: Vec<Tool>,
}

impl DeclaredToolsServer {
    pub fn new(quirk: Quirk) -> Self {
        let schema = |extra: usize| {
            let mut schema = JsonObject::new();
            schema.insert("type".into(), json!("object"));
            schema.insert("description".into(), json!("x".repeat(extra)));
            Arc::new(schema)
        };
        let tool = |name: String, extra: usize| Tool::new(name, "Lit un élément.", schema(extra));
        let tools = match quirk {
            Quirk::CollidingTools => vec![tool("get-item".into(), 0), tool("get_item".into(), 0)],
            Quirk::TooManyTools => (0..=MAX_TOOLS)
                .map(|i| tool(format!("tool_{i}"), 0))
                .collect(),
            Quirk::HugeSchema => vec![tool("get_item".into(), 40 * 1024)],
            Quirk::EchoesTokenInResults => vec![
                tool("echo_text".into(), 0),
                tool("echo_structured".into(), 0),
            ],
            _ => vec![tool("get_item".into(), 0)],
        };
        Self { tools }
    }
}

impl ServerHandler for DeclaredToolsServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("declared-tools", "0.1.0"))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let authorization = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.headers.get(header::AUTHORIZATION))
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let result = match request.name.as_ref() {
            "echo_text" => {
                CallToolResult::success(vec![ContentBlock::text(format!("reçu : {authorization}"))])
            }
            "echo_structured" => {
                CallToolResult::structured(json!({ "reçu": authorization, authorization: 1 }))
            }
            _ => CallToolResult::success(vec![ContentBlock::text("ok")]),
        };
        Ok(result.into())
    }
}

/// Checks the bearer token (a spec-conforming 401 otherwise), then applies the quirk.
pub(super) async fn guarded(
    expected: Option<String>,
    quirk: Quirk,
    request: Request,
    next: Next,
) -> Response {
    let given = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    if let Some(expected) = expected
        && given.as_deref() != Some(expected.as_str())
    {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(header::WWW_AUTHENTICATE, "Bearer realm=\"mcp\"")
            .body(Body::empty())
            .expect("response");
    }
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, 1 << 20).await.unwrap_or_default();
    let is_call = String::from_utf8_lossy(&body).contains("\"tools/call\"");
    if quirk == Quirk::EchoesTokenOnStart || (quirk == Quirk::EchoesTokenOnCall && is_call) {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(format!(
                "invalid credentials: {}",
                given.unwrap_or_default()
            )))
            .expect("response");
    }
    next.run(Request::from_parts(parts, Body::from(body))).await
}
