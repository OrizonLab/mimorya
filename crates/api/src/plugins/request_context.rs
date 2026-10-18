//! Per-request context: an identifier (written to the audit log) and the
//! client address, plus one log line per request. Headers are never logged
//! (cookies, credentials).

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::time::Instant;

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::app::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestContext {
    pub id: Uuid,
    pub ip: Option<IpAddr>,
}

impl RequestContext {
    pub fn ip_string(&self) -> Option<String> {
        self.ip.map(|ip| ip.to_string())
    }

    /// Key used by the rate limiters.
    pub fn client_key(&self) -> String {
        self.ip_string().unwrap_or_else(|| "unknown".into())
    }
}

/// With one trusted relay in front (the web server), the client is the last
/// address appended to `X-Forwarded-For`; earlier entries are client-supplied.
fn client_ip(request: &Request, trust_proxy: bool) -> Option<IpAddr> {
    let forwarded = trust_proxy
        .then(|| request.headers().get("x-forwarded-for"))
        .flatten()
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .and_then(|v| v.trim().parse().ok());
    forwarded.or_else(|| {
        request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip())
    })
}

pub async fn request_context(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let context = RequestContext {
        id: Uuid::new_v4(),
        ip: client_ip(&request, state.config.trust_proxy),
    };
    request.extensions_mut().insert(context);
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let started = Instant::now();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let duration_ms = started.elapsed().as_millis();
    if path == "/health" {
        tracing::debug!(request_id = %context.id, %method, path, status, duration_ms, "request");
    } else {
        tracing::info!(request_id = %context.id, %method, path, status, duration_ms, "request");
    }
    response
}

impl<S: Send + Sync> FromRequestParts<S> for RequestContext {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<RequestContext>()
            .copied()
            .unwrap_or(RequestContext {
                id: Uuid::new_v4(),
                ip: None,
            }))
    }
}
