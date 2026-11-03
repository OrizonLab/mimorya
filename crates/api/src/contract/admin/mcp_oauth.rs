//! OAuth declared on an MCP server (`connector:manage`, ADR-171): what the
//! administrator writes, and what the platform found and keeps. The
//! client's secret is never returned, only whether one is stored.

use mimorya_connectors::mcp::oauth::{ClientAuth, OAuthDeclaration};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::http::{trimmed, trimmed_opt};

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
    /// The client registered at the authorization server.
    #[serde(deserialize_with = "trimmed")]
    pub client_id: String,
    /// Absent or blank: a public client.
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[ts(optional)]
    pub client_secret: Option<String>,
    /// Absent: those the server suggests.
    #[serde(default)]
    #[ts(optional)]
    pub scopes: Option<Vec<String>>,
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
}

impl McpOAuth {
    pub fn new(declaration: OAuthDeclaration, client_secret_stored: bool) -> Self {
        Self {
            issuer: declaration.issuer,
            resource: declaration.resource,
            authorization_endpoint: declaration.authorization_endpoint,
            token_endpoint: declaration.token_endpoint,
            client_id: declaration.client_id,
            client_auth: declaration.client_auth.into(),
            scopes: declaration.scopes,
            client_secret_stored,
        }
    }
}
