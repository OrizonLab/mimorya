//! OAuth declared on an MCP server (`connector:manage`, ADR-171): what the
//! administrator writes, who authorizes it (ADR-174), and what the platform
//! found and keeps. The
//! client's secret is never returned, only whether one is stored.

use chrono::{DateTime, Utc};
use mimorya_connectors::mcp::oauth::{ClientAuth, OAuthDeclaration, OAuthHolder};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use mimorya_db::repositories::connector_authorizations::ConnectorAuthorizationRow;

use crate::http::{iso, trimmed_opt};

/// Body of `PUT /admin/connectors/{id}/oauth`: declared again, everything
/// is replaced, the secret included.
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpOAuthDeclaration {
    /// The authorization server's issuer: needed when the server names
    /// several, or none.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub issuer: Option<String>,
    /// The client registered at the authorization server; absent when the
    /// platform registers its own (`register`).
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub client_id: Option<String>,
    /// Absent or blank: a public client.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub client_secret: Option<String>,
    /// The platform registers its client at the authorization server
    /// itself (RFC 7591, ADR-173): no identifier nor secret is then given.
    #[serde(default)]
    #[ts(optional)]
    pub register: Option<bool>,
    /// Absent: those the server suggests.
    #[serde(default)]
    #[ts(optional)]
    pub scopes: Option<Vec<String>>,
    /// Each person authorizes the server for themself (ADR-174); false, the
    /// company does; absent, as under the declaration replaced (the company
    /// for a first one, or after one that could not be read).
    #[serde(default)]
    #[ts(optional)]
    pub per_person: Option<bool>,
}

/// Body of `PUT /admin/connectors/{id}/oauth/holder`: who authorizes the
/// server, changed without declaring it again (ADR-174).
#[derive(Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpOAuthHolder {
    pub per_person: bool,
}

/// How the client proves itself to the token endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum OAuthClientAuth {
    /// A public client: its identifier only.
    None,
    ClientSecretBasic,
    ClientSecretPost,
}

impl From<ClientAuth> for OAuthClientAuth {
    fn from(auth: ClientAuth) -> Self {
        match auth {
            ClientAuth::None => Self::None,
            ClientAuth::ClientSecretBasic => Self::ClientSecretBasic,
            ClientAuth::ClientSecretPost => Self::ClientSecretPost,
        }
    }
}

/// OAuth declared on an MCP server, as `McpEndpoint` shows it and its
/// declaration answers it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuth {
    pub issuer: String,
    /// The resource tokens are asked for.
    pub resource: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub client_id: String,
    pub client_auth: OAuthClientAuth,
    pub scopes: Vec<String>,
    pub client_secret_stored: bool,
    /// Whether the platform registered this client itself (ADR-173).
    pub registered: bool,
    /// When the secret of the client the platform registered expires, if
    /// it does.
    pub client_secret_expires_at: Option<String>,
    /// Whether its authorization server names a revocation endpoint, where
    /// the tokens the platform lets go of are revoked (ADR-172).
    pub revocable: bool,
    /// The authorization given under this declaration, if any.
    pub authorization: Option<McpOAuthAuthorization>,
    /// Whether each person authorizes the server for themself (ADR-174),
    /// rather than the company.
    pub per_person: bool,
    /// How many people authorized it for themselves: never who.
    pub person_authorizations: i64,
}

/// An MCP server's authorization, as its declaration shows it: never its
/// tokens.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthAuthorization {
    pub authorized_at: String,
    /// When the access token expires, if it does.
    pub expires_at: Option<String>,
    /// Whether a refresh token is kept.
    pub renewable: bool,
    pub scopes: Vec<String>,
    /// When its access token was last renewed, if it was since it was given.
    pub renewed_at: Option<String>,
    /// Why its last renewal failed, the authorization kept (never a token):
    /// tried again, cleared once renewed.
    pub renewal_failure: Option<String>,
}

impl From<ConnectorAuthorizationRow> for McpOAuthAuthorization {
    fn from(row: ConnectorAuthorizationRow) -> Self {
        Self {
            authorized_at: iso(&row.authorized_at),
            expires_at: row.access_expires_at.as_ref().map(iso),
            renewable: row.renewable,
            scopes: row.scopes.split_whitespace().map(String::from).collect(),
            renewed_at: (row.refreshed_at > row.authorized_at).then(|| iso(&row.refreshed_at)),
            renewal_failure: row.renewal_failure,
        }
    }
}

/// Answer of `POST /admin/connectors/{id}/oauth/authorization`: where to
/// send the browser; the attempt goes in a cookie.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OAuthAuthorizationStart {
    pub redirect_url: String,
}

impl McpOAuth {
    pub fn new(
        declaration: OAuthDeclaration,
        client_secret_stored: bool,
        authorization: Option<ConnectorAuthorizationRow>,
        person_authorizations: i64,
    ) -> Self {
        Self {
            per_person: declaration.holder == OAuthHolder::Person,
            person_authorizations,
            issuer: declaration.issuer,
            resource: declaration.resource,
            authorization_endpoint: declaration.authorization_endpoint,
            token_endpoint: declaration.token_endpoint,
            client_id: declaration.client_id,
            client_auth: declaration.client_auth.into(),
            revocable: declaration.revocation_endpoint.is_some(),
            client_secret_expires_at: declaration
                .registered
                .as_ref()
                .and_then(|registered| registered.secret_expires_at)
                .and_then(|at| DateTime::<Utc>::from_timestamp(i64::try_from(at).ok()?, 0))
                .as_ref()
                .map(iso),
            registered: declaration.registered.is_some(),
            scopes: declaration.scopes,
            client_secret_stored,
            authorization: authorization.map(Into::into),
        }
    }
}
