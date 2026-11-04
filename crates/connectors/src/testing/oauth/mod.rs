//! OAuth as an MCP server's authorization would be (ADR-171): an
//! authorization server (`authorization_server`, its token endpoint in
//! `token_endpoint`) and the demo documentation server protected by its
//! tokens (`protected`), hostile at will (`hostile`).

mod authorization_server;
mod hostile;
mod protected;
mod token_endpoint;

pub use authorization_server::{
    AuthRequest, CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, LONG_TOKEN_CHARS, Settings,
};
pub use hostile::LURE;
pub use protected::{METADATA_PATH, OAuthQuirk, ProtectedMcp, SLOW_ANSWER, serve_oauth_mcp_http};
