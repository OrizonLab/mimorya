//! OAuth as an MCP server's authorization would be (ADR-171): an
//! authorization server (`authorization_server`) and the demo documentation
//! server protected by its tokens (`protected`).

mod authorization_server;
mod protected;

pub use authorization_server::{
    AuthRequest, CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, Settings,
};
pub use protected::{METADATA_PATH, OAuthQuirk, ProtectedMcp, serve_oauth_mcp_http};
