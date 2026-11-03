//! The demo documentation server as an OAuth-protected resource (RFC 9728):
//! it takes only a token the fake authorization server issued for its very
//! address, refuses any other request with a `Bearer` challenge naming its
//! metadata, and publishes that metadata on its own origin. Its quirks are
//! what a server may get wrong, or what an attacker may try; it records the
//! `Authorization` header of every request it receives.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;

use super::authorization_server::FakeAuthorizationServer;
use crate::mcp::DemoDocsServer;
use crate::testing::mcp_router;

/// Where the server publishes its metadata, under its path (RFC 9728 §3.1).
pub const METADATA_PATH: &str = "/.well-known/oauth-protected-resource/mcp";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OAuthQuirk {
    #[default]
    None,
    /// No metadata, and a challenge that names none.
    NoMetadata,
    /// A challenge naming metadata on another origin.
    ForeignMetadata,
    /// Metadata naming another resource than the server.
    OtherResource,
    /// Metadata naming two authorization servers.
    TwoServers,
    /// Every token refused, even a good one.
    RefusesAll,
}

/// The server: its address, and the `Authorization` headers it received.
pub struct ProtectedMcp {
    pub url: String,
    seen: Arc<Mutex<Vec<Option<String>>>>,
}

impl ProtectedMcp {
    /// The `Authorization` header of each request, in order (`None`:
    /// without one).
    pub fn authorizations(&self) -> Vec<Option<String>> {
        self.seen.lock().expect("seen").clone()
    }
}

/// Serves the protected server until the process ends, its tokens issued
/// by `authority`.
pub async fn serve_oauth_mcp_http(
    authority: &FakeAuthorizationServer,
    quirk: OAuthQuirk,
) -> ProtectedMcp {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("local port");
    let origin = format!("http://{}", listener.local_addr().expect("address"));
    let url = format!("{origin}/mcp");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let challenge = match quirk {
        OAuthQuirk::NoMetadata => "Bearer realm=\"mcp\"".to_string(),
        OAuthQuirk::ForeignMetadata => format!(
            "Bearer error=\"invalid_token\", resource_metadata=\"http://127.0.0.2:9{METADATA_PATH}\""
        ),
        _ => format!(
            "Bearer error=\"invalid_token\", resource_metadata=\"{origin}{METADATA_PATH}\", \
             scope=\"files:read\""
        ),
    };
    let guard = {
        let (authority, url, seen) = (authority.clone(), url.clone(), seen.clone());
        move |request: Request, next: Next| {
            let (authority, url, seen, challenge) = (
                authority.clone(),
                url.clone(),
                seen.clone(),
                challenge.clone(),
            );
            async move {
                let given = request
                    .headers()
                    .get(header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                seen.lock().expect("seen").push(given.clone());
                let token = given.as_deref().and_then(|v| v.strip_prefix("Bearer "));
                if quirk != OAuthQuirk::RefusesAll
                    && token.is_some_and(|t| authority.accepts(t, &url))
                {
                    return next.run(request).await;
                }
                Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .header(header::WWW_AUTHENTICATE, challenge)
                    .body(Body::empty())
                    .expect("response")
            }
        }
    };
    let mut router = mcp_router(DemoDocsServer::default).route_layer(middleware::from_fn(guard));
    if quirk != OAuthQuirk::NoMetadata {
        let resource = if quirk == OAuthQuirk::OtherResource {
            format!("{url}/autre")
        } else {
            url.clone()
        };
        let mut servers = vec![authority.issuer.clone()];
        if quirk == OAuthQuirk::TwoServers {
            servers.push("https://autre-serveur.example".into());
        }
        let document = json!({
            "resource": resource,
            "authorization_servers": servers,
            "scopes_supported": ["files:read"],
            "bearer_methods_supported": ["header"],
        });
        router = router.route(
            METADATA_PATH,
            get(move || {
                let document = document.clone();
                async move { axum::Json(document).into_response() }
            }),
        );
    }
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test MCP server")
    });
    ProtectedMcp { url, seen }
}
