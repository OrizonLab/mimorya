//! OAuth for MCP servers (ADR-171), as the MCP authorization specification
//! asks: the server, a protected resource, names where its tokens are
//! issued (Protected Resource Metadata, RFC 9728, on its own origin only,
//! `resource`, `protected_resource`, the challenge of its refusals,
//! `challenge`); the authorization server says how to ask (RFC 8414,
//! `server_metadata`); what is asked for is checked (`scope`). `discovery`
//! follows these rules through the hardened client (`fetch`), once, when an
//! administrator declares OAuth on a server; what is kept of it is the
//! declaration (`declaration`). An administrator's browser is then sent to
//! authorize it (`departure`), and the code it brings back exchanged for
//! tokens (`exchange`), at the token endpoint (`token_request`) that later
//! renews them with the refresh token (`refresh`).

mod challenge;
mod declaration;
mod departure;
mod discovery;
mod exchange;
mod fetch;
mod protected_resource;
mod quoted;
mod refresh;
mod resource;
mod scope;
mod server_metadata;
mod token_request;

pub use challenge::{BearerChallenge, MAX_HEADER_CHARS, bearer_challenge};
pub use declaration::{
    MAX_CLIENT_ID_CHARS, MAX_CLIENT_SECRET_CHARS, OAUTH_TO_AUTHORIZE, OAuthDeclaration,
    client_id_problem, client_secret_problem,
};
pub use departure::{Departure, authorization_url};
pub use discovery::{Discovery, DiscoveryLimits, discover, discover_within};
pub use exchange::{
    CodeExchange, Granted, MAX_OAUTH_TOKEN_CHARS, access_token_problem, exchange_code,
};
pub use protected_resource::{
    MAX_AUTHORIZATION_SERVERS, ProtectedResource, endpoint_problem, issuer_problem,
    read_protected_resource,
};
pub use refresh::{RenewalFailure, refresh_tokens};
pub use resource::{
    canonical_resource, covers, protected_resource_locations, same_origin, same_resource,
};
pub use scope::{MAX_SCOPE_CHARS, MAX_SCOPES, scope_problem, scopes_problem, suggested_scopes};
pub use server_metadata::{
    AuthorizationServer, ClientAuth, metadata_locations, read_server_metadata,
};
