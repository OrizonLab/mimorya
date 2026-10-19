//! Test support (feature `testing`): MCP servers over Streamable HTTP on a
//! local port, optionally behind a bearer token, and misbehaving ones (what
//! they declare, a token quoted back, a server that never answers).

mod quirks;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::middleware::{self, Next};
use rmcp::ServerHandler;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::mcp::DemoDocsServer;
pub use quirks::{DeclaredToolsServer, Quirk};

/// Serves the demo documentation server at `http://127.0.0.1:<port>/mcp`
/// until the process ends; returns that address.
pub async fn serve_demo_docs_http(token: Option<&str>) -> String {
    serve_mcp_http(token, Quirk::None).await
}

/// Serves an MCP server with the given quirk; returns its address.
pub async fn serve_mcp_http(token: Option<&str>, quirk: Quirk) -> String {
    serve_mcp_http_on(0, token, quirk).await
}

/// The same, on a given local port (a server coming back where it was).
pub async fn serve_mcp_http_on(port: u16, token: Option<&str>, quirk: Quirk) -> String {
    let router = match quirk {
        Quirk::None
        | Quirk::SlowStart
        | Quirk::EchoesTokenOnStart
        | Quirk::EchoesEscapedTokenOnStart
        | Quirk::EchoesTokenOnCall => mcp_router(DemoDocsServer::default),
        _ => mcp_router(move || DeclaredToolsServer::new(quirk)),
    };
    let expected = token.map(|t| format!("Bearer {t}"));
    let router = router.layer(middleware::from_fn(move |request: Request, next: Next| {
        let expected = expected.clone();
        async move { quirks::guarded(expected, quirk, request, next).await }
    }));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("local port");
    let address: SocketAddr = listener.local_addr().expect("local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test MCP server");
    });
    format!("http://{address}/mcp")
}

/// An address that accepts connections and never answers.
pub async fn serve_mute() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("local port");
    let address: SocketAddr = listener.local_addr().expect("local address");
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            held.push(socket);
        }
    });
    format!("http://{address}/mcp")
}

fn mcp_router<S>(server: impl Fn() -> S + Send + Sync + 'static) -> Router
where
    S: ServerHandler + Send + 'static,
{
    let service = StreamableHttpService::new(
        move || Ok(server()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    Router::new().nest_service("/mcp", service)
}
