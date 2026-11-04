//! The demo documentation server as an OAuth-protected resource (RFC 9728):
//! it takes only a token the fake authorization server issued for its very
//! address, refuses any other request with a `Bearer` challenge naming its
//! metadata, and publishes that metadata on its own origin. Its quirks are
//! what a server may get wrong, or what an attacker may try (those that
//! test the bounds of the discovery: `hostile`); it records the
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
use super::hostile::{endless_body, hostile_routes, lure_path};
use crate::mcp::DemoDocsServer;
use crate::testing::{Hold, mcp_router};

/// Where the server publishes its metadata, under its path (RFC 9728 §3.1).
pub const METADATA_PATH: &str = "/.well-known/oauth-protected-resource/mcp";
/// Where a server publishes it for its whole origin.
const ROOT_METADATA_PATH: &str = "/.well-known/oauth-protected-resource";

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
    /// The refusal's challenges on two header lines, `Basic` then `Bearer`.
    SplitChallenge,
    /// Metadata at the root only, naming the origin as the resource
    /// (RFC 9728 §3.3), and a challenge that names none.
    RootMetadata,
    /// Metadata under its path naming the origin as the resource.
    OriginAtPath,
    /// No metadata of its own, but its origin publishes that of an
    /// authorization server whose issuer it is (`hostile`).
    OwnAuthority,
    /// Metadata longer than any document is (`hostile`).
    HugeMetadata,
    /// Metadata that never comes (`hostile`).
    SilentMetadata,
    /// Metadata whose answer breaks off, at a long address the challenge
    /// names (`hostile`).
    BrokenMetadata,
    /// Metadata answered 503 (`hostile`).
    UnavailableMetadata,
    /// Metadata whose answer starts, then never ends (`hostile`).
    DripMetadata,
    /// A request without a token answered with this status instead of a
    /// refusal.
    ProbeAnswered(u16),
    /// A refusal whose body never ends.
    EndlessRefusal,
    /// A challenge naming its metadata with parameters, which the
    /// platform does not send.
    ParamsInChallenge,
    /// Its metadata answered only once the test releases `hold`.
    HeldMetadata,
    /// Every request answered after `SLOW_ANSWER`: a session long to start.
    SlowStart,
}

/// How long `SlowStart` takes to answer each request.
pub const SLOW_ANSWER: std::time::Duration = std::time::Duration::from_secs(1);

impl OAuthQuirk {
    /// Whether the server publishes the usual metadata document.
    fn publishes_metadata(self) -> bool {
        !matches!(
            self,
            Self::NoMetadata
                | Self::OwnAuthority
                | Self::HugeMetadata
                | Self::SilentMetadata
                | Self::BrokenMetadata
                | Self::UnavailableMetadata
                | Self::DripMetadata
        )
    }
}

/// The server: its address, and the `Authorization` headers it received.
pub struct ProtectedMcp {
    pub url: String,
    /// What holds its metadata, with `HeldMetadata`.
    pub hold: Arc<Hold>,
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
    serve_oauth_mcp_http_on(authority, quirk, "127.0.0.1:0").await
}

/// `serve_oauth_mcp_http`, listening at `address` (end-to-end tests: a known
/// port).
pub async fn serve_oauth_mcp_http_on(
    authority: &FakeAuthorizationServer,
    quirk: OAuthQuirk,
    address: &str,
) -> ProtectedMcp {
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("local port");
    let origin = format!("http://{}", listener.local_addr().expect("address"));
    let url = format!("{origin}/mcp");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let hold = Arc::new(Hold::default());
    let challenge = match quirk {
        OAuthQuirk::NoMetadata | OAuthQuirk::RootMetadata | OAuthQuirk::OwnAuthority => {
            "Bearer realm=\"mcp\"".to_string()
        }
        OAuthQuirk::ForeignMetadata => format!(
            "Bearer error=\"invalid_token\", resource_metadata=\"http://127.0.0.2:9{METADATA_PATH}\", \
             scope=\"files:read\""
        ),
        OAuthQuirk::BrokenMetadata => format!(
            "Bearer error=\"invalid_token\", resource_metadata=\"{origin}{}\"",
            lure_path()
        ),
        OAuthQuirk::ParamsInChallenge => format!(
            "Bearer error=\"invalid_token\", resource_metadata=\"{origin}{METADATA_PATH}?v=2\""
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
                if quirk == OAuthQuirk::SlowStart {
                    tokio::time::sleep(SLOW_ANSWER).await;
                }
                let token = given.as_deref().and_then(|v| v.strip_prefix("Bearer "));
                if quirk != OAuthQuirk::RefusesAll
                    && token.is_some_and(|t| authority.accepts(t, &url))
                {
                    return next.run(request).await;
                }
                if let OAuthQuirk::ProbeAnswered(status) = quirk {
                    let status = StatusCode::from_u16(status).expect("status");
                    return status.into_response();
                }
                let mut refusal = Response::builder().status(StatusCode::UNAUTHORIZED);
                if quirk == OAuthQuirk::SplitChallenge {
                    refusal = refusal.header(header::WWW_AUTHENTICATE, "Basic realm=\"mcp\"");
                }
                let body = if quirk == OAuthQuirk::EndlessRefusal {
                    endless_body()
                } else {
                    Body::empty()
                };
                refusal
                    .header(header::WWW_AUTHENTICATE, challenge)
                    .body(body)
                    .expect("response")
            }
        }
    };
    let mut router = mcp_router(DemoDocsServer::default).route_layer(middleware::from_fn(guard));
    router = hostile_routes(router, quirk, &origin, authority);
    if quirk.publishes_metadata() {
        let resource = match quirk {
            OAuthQuirk::OtherResource => format!("{url}/autre"),
            OAuthQuirk::RootMetadata | OAuthQuirk::OriginAtPath => origin.clone(),
            _ => url.clone(),
        };
        let mut servers = vec![authority.issuer.clone()];
        if quirk == OAuthQuirk::TwoServers {
            servers.push("https://autre-serveur.example".into());
        }
        let document = json!({
            "resource": resource,
            "authorization_servers": servers,
            "scopes_supported": ["files:read", "files:write"],
            "bearer_methods_supported": ["header"],
        });
        let path = if quirk == OAuthQuirk::RootMetadata {
            ROOT_METADATA_PATH
        } else {
            METADATA_PATH
        };
        let held = (quirk == OAuthQuirk::HeldMetadata).then(|| hold.clone());
        router = router.route(
            path,
            get(move || {
                let (document, held) = (document.clone(), held.clone());
                async move {
                    if let Some(hold) = held {
                        hold.wait().await;
                    }
                    axum::Json(document).into_response()
                }
            }),
        );
    }
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test MCP server")
    });
    ProtectedMcp { url, hold, seen }
}
