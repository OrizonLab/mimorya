//! What every connector a company connects itself shares, MCP server
//! (ADR-025) or REST API (ADR-112): the permission all its tools require,
//! the prefix naming them, how its server is reached (the operator's network
//! policy and proxy, over HTTP or, for its mail servers, a connection of
//! its own secured by TLS, ADR-205), what its token may be, how that token
//! is masked in whatever the server sends back, and how a failure to reach
//! it is told.

mod bounded;
mod failure;
mod network;
mod prefix;
mod proxy;
mod redact;
mod tcp;
mod tls;
mod token;
mod tunnel;

pub use bounded::{ReadBudget, TOO_LONG, bounded};
pub use failure::{
    ACCESS_DENIED, Cause, NOT_PUBLIC, TOO_SLOW, UNREACHABLE, explained, request_cause, too_slow,
};
pub use network::{MAX_URL_CHARS, NetworkPolicy, http_client, is_public_ip, url_problem};
pub use prefix::prefix_problem;
pub use proxy::OutboundProxy;
pub use redact::{MASK, Redactor};
pub use tcp::{Connection, INVALID_HOST, MAX_HOST_CHARS, Stream, connect, host_problem};
pub use tls::{NOT_SECURE, client_config, secured};
pub use token::{MAX_TOKEN_CHARS, MIN_TOKEN_CHARS, token_problem};
pub use tunnel::PROXY_REFUSED;

/// Every tool of a server a company connects requires it, whatever its
/// policy says (ADR-025).
pub const CONNECTOR_USE: &str = "connector:use";
