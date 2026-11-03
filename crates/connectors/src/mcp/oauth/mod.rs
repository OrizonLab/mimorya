//! OAuth for MCP servers (ADR-171), as the MCP authorization specification
//! asks: the server, a protected resource, names where its tokens are
//! issued (Protected Resource Metadata, RFC 9728, on its own origin only,
//! `resource`, `protected_resource`, the challenge of its refusals,
//! `challenge`); the authorization server says how to ask (RFC 8414,
//! `server_metadata`); what is asked for is checked (`scope`). These are
//! the rules, without any exchange: the platform's services fetch the
//! documents through the hardened client and keep the result.

mod challenge;
mod protected_resource;
mod quoted;
mod resource;
mod scope;
mod server_metadata;

pub use challenge::{BearerChallenge, MAX_HEADER_CHARS, bearer_challenge};
pub use protected_resource::{
    MAX_AUTHORIZATION_SERVERS, ProtectedResource, endpoint_problem, issuer_problem,
    read_protected_resource,
};
pub use resource::{canonical_resource, protected_resource_locations, same_origin, same_resource};
pub use scope::{MAX_SCOPE_CHARS, MAX_SCOPES, scope_problem, scopes_problem, suggested_scopes};
pub use server_metadata::{
    AuthorizationServer, ClientAuth, metadata_locations, read_server_metadata,
};
