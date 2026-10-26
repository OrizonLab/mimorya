//! What every connector a company connects itself shares, MCP server
//! (ADR-025) or REST API (ADR-112): the permission all its tools require,
//! the prefix naming them, how its server is reached (the operator's network
//! policy), what its token may be, how that token is masked in whatever
//! the server sends back, and how a failure to reach it is told.

mod failure;
mod network;
mod prefix;
mod redact;
mod token;

pub use failure::{
    ACCESS_DENIED, Cause, NOT_PUBLIC, TOO_SLOW, UNREACHABLE, explained, request_cause,
};
pub use network::{MAX_URL_CHARS, NetworkPolicy, http_client, is_public_ip, url_problem};
pub use prefix::prefix_problem;
pub use redact::{MASK, Redactor};
pub use token::{MAX_TOKEN_CHARS, MIN_TOKEN_CHARS, token_problem};

/// Every tool of a server a company connects requires it, whatever its
/// policy says (ADR-025).
pub const CONNECTOR_USE: &str = "connector:use";
