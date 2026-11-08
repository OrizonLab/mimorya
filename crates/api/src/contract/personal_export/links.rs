//! What the person linked or authorized, in their export (ADR-187): the
//! accounts, messaging identities and MCP servers, each by its name and
//! who it says the person is; never a token.

use mimorya_db::repositories::personal_data::{
    ExportedAuthorization, ExportedIdentity, ExportedLink,
};
use serde::Serialize;
use ts_rs::TS;

use crate::http::iso;

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportLink {
    pub connector: String,
    /// Who the provider says the account is.
    pub subject: String,
    pub address: String,
    pub scopes: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportIdentity {
    /// `slack`, `telegram` or `teams`.
    pub kind: String,
    pub gateway: String,
    /// The person's identifier in the messaging service.
    pub external_id: String,
    pub display_name: Option<String>,
    pub address: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportAuthorization {
    pub connector: String,
    pub scopes: String,
    pub authorized_at: String,
}

impl From<ExportedLink> for ExportLink {
    fn from(link: ExportedLink) -> Self {
        Self {
            connector: link.connector,
            subject: link.subject,
            address: link.address,
            scopes: link.scopes,
            created_at: iso(&link.created_at),
        }
    }
}

impl From<ExportedIdentity> for ExportIdentity {
    fn from(identity: ExportedIdentity) -> Self {
        Self {
            kind: identity.kind,
            gateway: identity.gateway,
            external_id: identity.external_id,
            display_name: identity.display_name,
            address: identity.address,
            created_at: iso(&identity.created_at),
        }
    }
}

impl From<ExportedAuthorization> for ExportAuthorization {
    fn from(authorization: ExportedAuthorization) -> Self {
        Self {
            connector: authorization.connector,
            scopes: authorization.scopes,
            authorized_at: iso(&authorization.authorized_at),
        }
    }
}
