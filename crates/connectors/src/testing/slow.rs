//! A test MCP server whose tool is slow (ADR-164): `get_item` answers after
//! `SLOW_CALL`, `refused_item` refuses at once with an error of its own. It
//! answers a ping after `PING_DELAY`, refuses it as a method it does not
//! know, or refuses it with a 429, as chosen; it counts the pings it saw.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::Response;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData,
    Implementation, JsonObject, ListToolsResult, PaginatedRequestParams, PingRequestMethod,
    ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler};

/// How long `get_item` takes.
pub const SLOW_CALL: Duration = Duration::from_secs(3);

/// How long an answered ping takes: long enough for failures that come
/// together to find it under way.
pub const PING_DELAY: Duration = Duration::from_millis(500);

/// How the server answers a ping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ping {
    Answered,
    /// Refused as a method it does not know.
    Unknown,
    /// Refused with a 429, before reaching the MCP server.
    RateLimited,
}

/// What the server saw.
#[derive(Debug, Default)]
pub struct Seen {
    pings: AtomicUsize,
}

impl Seen {
    pub fn pings(&self) -> usize {
        self.pings.load(Ordering::SeqCst)
    }
}

/// Serves the slow server at `http://127.0.0.1:<port>/mcp`; returns its
/// address and what it sees.
pub async fn serve_slow_mcp_http(ping: Ping) -> (String, Arc<Seen>) {
    let seen = Arc::new(Seen::default());
    let server = SlowServer {
        ping,
        seen: seen.clone(),
    };
    let counted = seen.clone();
    let router = super::mcp_router(move || server.clone()).layer(middleware::from_fn(
        move |request: Request, next: Next| {
            let seen = counted.clone();
            async move { rate_limited(ping, &seen, request, next).await }
        },
    ));
    (super::serve(router, 0).await, seen)
}

/// A ping refused with a 429 when the server limits them.
async fn rate_limited(ping: Ping, seen: &Seen, request: Request, next: Next) -> Response {
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, 1 << 20).await.unwrap_or_default();
    if ping == Ping::RateLimited && String::from_utf8_lossy(&body).contains("\"ping\"") {
        seen.pings.fetch_add(1, Ordering::SeqCst);
        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .body(Body::from("Too Many Requests"))
            .expect("response");
    }
    next.run(Request::from_parts(parts, Body::from(body))).await
}

#[derive(Clone)]
struct SlowServer {
    ping: Ping,
    seen: Arc<Seen>,
}

impl ServerHandler for SlowServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("slow", "0.1.0"))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tool =
            |name: &'static str| Tool::new(name, "Lit un élément.", Arc::new(JsonObject::new()));
        let tools = vec![tool("get_item"), tool("refused_item")];
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn ping(&self, _context: RequestContext<RoleServer>) -> Result<(), ErrorData> {
        self.seen.pings.fetch_add(1, Ordering::SeqCst);
        match self.ping {
            Ping::Unknown => Err(ErrorData::method_not_found::<PingRequestMethod>()),
            _ => {
                tokio::time::sleep(PING_DELAY).await;
                Ok(())
            }
        }
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        match request.name.as_ref() {
            "refused_item" => Err(ErrorData::invalid_request("Élément refusé", None)),
            _ => {
                tokio::time::sleep(SLOW_CALL).await;
                Ok(CallToolResult::success(vec![ContentBlock::text("ok")]).into())
            }
        }
    }
}
