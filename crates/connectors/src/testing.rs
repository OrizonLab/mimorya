//! Test support (feature `testing`): the demo documentation server over
//! Streamable HTTP on a local port, optionally behind a bearer token.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::http::{StatusCode, header::AUTHORIZATION};
use axum::middleware::{self, Next};
use axum::response::Response;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::mcp::DemoDocsServer;

/// Serves the demo documentation server at `http://127.0.0.1:<port>/mcp`
/// until the process ends; returns that address.
pub async fn serve_demo_docs_http(token: Option<&str>) -> String {
    let service = StreamableHttpService::new(
        || Ok(DemoDocsServer::default()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let expected = token.map(|t| format!("Bearer {t}"));
    let router = Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn(move |request: Request, next: Next| {
            let expected = expected.clone();
            async move { authorized(expected, request, next).await }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("local port");
    let address: SocketAddr = listener.local_addr().expect("local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("demo MCP server");
    });
    format!("http://{address}/mcp")
}

async fn authorized(expected: Option<String>, request: Request, next: Next) -> Response {
    let given = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    match expected {
        Some(expected) if given != Some(expected.as_str()) => Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(axum::body::Body::empty())
            .expect("response"),
        _ => next.run(request).await,
    }
}
