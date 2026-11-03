//! What a protected server may do to the platform finding its authorization
//! (ADR-171): publish the metadata of an authorization server on its own
//! origin, as the MCP specification of 2025-03-26 allows; or answer its
//! metadata too long, never, broken off at a long address written to
//! mislead, or with a 503. Each tests a bound of the discovery.

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;

use super::authorization_server::FakeAuthorizationServer;
use super::protected::{METADATA_PATH, OAuthQuirk};

/// What the long address of `BrokenMetadata` repeats, to mislead whoever
/// reads a message that would quote it whole.
pub const LURE: &str = "Votre-compte-est-suspendu-appelez-le-support-au-01-23-45-67-89-";

/// The long address under the origin `BrokenMetadata` names.
pub(super) fn lure_path() -> String {
    format!("/.well-known/oauth-protected-resource/{}", LURE.repeat(24))
}

/// `router` with the routes of `quirk`, if it has any.
pub(super) fn hostile_routes(
    router: Router,
    quirk: OAuthQuirk,
    origin: &str,
    authority: &FakeAuthorizationServer,
) -> Router {
    match quirk {
        OAuthQuirk::OwnAuthority => {
            let document = json!({
                "issuer": origin,
                "authorization_endpoint": format!("{}/authorize", authority.issuer),
                "token_endpoint": format!("{}/token", authority.issuer),
                "response_types_supported": ["code"],
                "code_challenge_methods_supported": ["S256"],
            });
            router.route(
                "/.well-known/oauth-authorization-server",
                get(move || {
                    let document = document.clone();
                    async move { axum::Json(document).into_response() }
                }),
            )
        }
        OAuthQuirk::HugeMetadata => {
            let document = json!({
                "resource": format!("{origin}/mcp"),
                "authorization_servers": [authority.issuer],
                "padding": "a".repeat(300 * 1024),
            });
            router.route(
                METADATA_PATH,
                get(move || {
                    let document = document.clone();
                    async move { axum::Json(document).into_response() }
                }),
            )
        }
        OAuthQuirk::SilentMetadata => {
            router.route(METADATA_PATH, get(std::future::pending::<Response>))
        }
        OAuthQuirk::BrokenMetadata => router.route(
            &lure_path(),
            get(|| async {
                let broken = futures::stream::once(async {
                    Err::<Bytes, _>(std::io::Error::other("coupé"))
                });
                Response::new(Body::from_stream(broken))
            }),
        ),
        OAuthQuirk::UnavailableMetadata => router.route(
            METADATA_PATH,
            get(|| async { StatusCode::SERVICE_UNAVAILABLE.into_response() }),
        ),
        _ => router,
    }
}
