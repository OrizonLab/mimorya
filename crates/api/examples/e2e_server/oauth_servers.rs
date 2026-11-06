//! The MCP servers protected by OAuth that the end-to-end tests declare
//! and authorize, and their authorization servers, at known addresses: the
//! only private ones the connectors reach (`connector_test_servers`).
//!
//! - 4106 and 4107: `mcp-oauth.spec.ts` declares and authorizes it for the
//!   company (ADR-171), `mcp-person-oauth.spec.ts` and
//!   `mcp-person-oauth-declarations.spec.ts` for each person (ADR-174); the
//!   clients it registers have a secret lasting thirty days, which the card
//!   says (ADR-173);
//! - 4108 and 4109: the same, its authorization server naming no
//!   revocation endpoint (ADR-172);
//! - 4110 and 4111: an OpenID provider where Marc is signed in, its
//!   identity tokens proving his address: `mcp-person-oauth-identity.spec.ts`
//!   verifies who authorizes it (ADR-175).

use std::net::SocketAddr;

use mimorya_connectors::testing::{
    FakeAuthorizationServer, OAuthQuirk, SignedIn, serve_oauth_mcp_http_on,
};
use mimorya_core::demo::{self, user};

const OAUTH_AUTHORITY_ADDRESS: &str = "127.0.0.1:4106";
const OAUTH_MCP_ADDRESS: &str = "127.0.0.1:4107";
const UNREVOKING_AUTHORITY_ADDRESS: &str = "127.0.0.1:4108";
const UNREVOKING_MCP_ADDRESS: &str = "127.0.0.1:4109";
const IDENTITY_AUTHORITY_ADDRESS: &str = "127.0.0.1:4110";
const IDENTITY_MCP_ADDRESS: &str = "127.0.0.1:4111";

/// Starts them all, until the process ends: the addresses the connectors
/// may reach.
pub async fn start() -> anyhow::Result<Vec<SocketAddr>> {
    let authority = FakeAuthorizationServer::start_on(OAUTH_AUTHORITY_ADDRESS).await;
    authority.set(|settings| settings.secret_lifetime = Some(30 * 86_400));
    let protected = serve_oauth_mcp_http_on(&authority, OAuthQuirk::None, OAUTH_MCP_ADDRESS).await;
    tracing::info!(
        url = protected.url,
        "End-to-end MCP server protected by OAuth listening"
    );
    let unrevoking = FakeAuthorizationServer::start_on(UNREVOKING_AUTHORITY_ADDRESS).await;
    unrevoking.set(|settings| settings.revocation = false);
    let protected =
        serve_oauth_mcp_http_on(&unrevoking, OAuthQuirk::None, UNREVOKING_MCP_ADDRESS).await;
    tracing::info!(
        url = protected.url,
        "End-to-end MCP server protected by an authorization server that does not revoke"
    );
    let marc = demo::users()
        .into_iter()
        .find(|person| person.id == user::MARC)
        .ok_or_else(|| anyhow::anyhow!("no demo account for Marc"))?;
    let identity = FakeAuthorizationServer::start_on(IDENTITY_AUTHORITY_ADDRESS).await;
    identity.set(|settings| settings.identity = Some(SignedIn::verified(&marc.email)));
    let protected =
        serve_oauth_mcp_http_on(&identity, OAuthQuirk::None, IDENTITY_MCP_ADDRESS).await;
    tracing::info!(
        url = protected.url,
        "End-to-end MCP server protected by an OpenID provider where Marc is signed in"
    );
    let addresses = [
        OAUTH_AUTHORITY_ADDRESS,
        OAUTH_MCP_ADDRESS,
        UNREVOKING_AUTHORITY_ADDRESS,
        UNREVOKING_MCP_ADDRESS,
        IDENTITY_AUTHORITY_ADDRESS,
        IDENTITY_MCP_ADDRESS,
    ];
    Ok(addresses
        .iter()
        .map(|address| address.parse())
        .collect::<Result<_, _>>()?)
}
