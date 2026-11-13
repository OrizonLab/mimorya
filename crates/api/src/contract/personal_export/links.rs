//! What the person linked or authorized, in their export (ADR-187): the
//! accounts, messaging identities and MCP servers, each by its name and
//! who it says the person is, their account at the company's standard
//! mail by its identifier (ADR-205), and the messaging thread a
//! conversation is held in; never a token nor a password.

use mimorya_db::repositories::mail_accounts::ExportedMailAccount;
use mimorya_db::repositories::personal_content::ExportedThread;
use mimorya_db::repositories::personal_data::{
    ExportedAuthorization, ExportedIdentity, ExportedLink,
};
use serde::Serialize;
use ts_rs::TS;

use super::iso_opt;
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

/// Where a conversation is held in a messaging service: the person there,
/// the channel and the thread.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportThread {
    /// `slack`, `telegram` or `teams`.
    pub kind: String,
    pub gateway: String,
    pub external_id: String,
    pub channel: String,
    pub thread: String,
    pub created_at: String,
    pub active_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportAuthorization {
    pub connector: String,
    pub scopes: String,
    pub authorized_at: String,
}

/// The person's account at the company's standard mail, without its
/// password (ADR-205).
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportMailAccount {
    pub connector: String,
    pub username: String,
    pub linked_at: String,
}

impl From<ExportedMailAccount> for ExportMailAccount {
    fn from(account: ExportedMailAccount) -> Self {
        Self {
            connector: account.connector,
            username: account.username,
            linked_at: iso(&account.linked_at),
        }
    }
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

impl From<ExportedThread> for ExportThread {
    fn from(thread: ExportedThread) -> Self {
        Self {
            kind: thread.kind,
            gateway: thread.gateway,
            external_id: thread.external_id,
            channel: thread.channel_id,
            thread: thread.thread_ts,
            created_at: iso(&thread.created_at),
            active_at: iso(&thread.active_at),
            closed_at: iso_opt(thread.closed_at),
        }
    }
}
