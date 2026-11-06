//! OAuth as an MCP server's authorization would be (ADR-171): an
//! authorization server (`authorization_server`, its token endpoint in
//! `token_endpoint`, an OpenID provider once someone is signed in there,
//! `openid`, ADR-175) and the demo documentation server protected by its
//! tokens (`protected`), hostile at will (`hostile`).

mod authorization_server;
mod hostile;
mod metadata_endpoint;
mod openid;
mod protected;
mod registration_endpoint;
mod revocation_endpoint;
mod token_endpoint;

pub use authorization_server::{
    AuthRequest, CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, LONG_TOKEN_CHARS, Settings,
};
pub use hostile::LURE;
pub use openid::{IdTokenQuirk, SignedIn};
pub use protected::{
    METADATA_PATH, OAuthQuirk, ProtectedMcp, SLOW_ANSWER, serve_oauth_mcp_http,
    serve_oauth_mcp_http_on,
};
