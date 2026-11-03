//! OAuth as an MCP server's authorization would be (ADR-171): an
//! authorization server (`authorization_server`, its token endpoint in
//! `token_endpoint`) and the demo documentation server protected by its
//! tokens (`protected`).

mod authorization_server;
mod protected;
mod token_endpoint;

pub use authorization_server::{
    AuthRequest, CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, Settings,
};
pub use protected::{METADATA_PATH, OAuthQuirk, ProtectedMcp, serve_oauth_mcp_http};
