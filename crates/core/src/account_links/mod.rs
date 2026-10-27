//! Accounts a person links at a provider their company declared (ADR-116,
//! ADR-121): what a company declares of its Microsoft 365 or Google
//! Workspace application, the delegated permissions a person grants (never
//! an application permission, which would open every mailbox of the
//! directory or domain), and when a verified token proves the linked
//! account is the person's own. Each provider's rules are in its module;
//! `LinkKind` says which apply.

mod google;
mod kind;
mod microsoft365;

pub use google::{
    CALENDAR_EVENTS, CALENDAR_READONLY, GMAIL_READONLY, GMAIL_SEND, GOOGLE_CALENDAR, GOOGLE_GMAIL,
    GOOGLE_WORKSPACE, GoogleWorkspaceApplication, google_workspace_application,
};
pub use kind::LinkKind;
pub use microsoft365::{
    CALENDARS_READ, CALENDARS_WRITE, MAIL_READ, MAIL_SEND, MICROSOFT_GRAPH, MICROSOFT365,
    Microsoft365Application, microsoft365_application,
};
use uuid::Uuid;

/// The permissions granted, as kept: bounded like the column.
pub const MAX_SCOPES_CHARS: usize = 1000;
/// A provider's address, as kept: bounded like the column.
pub const MAX_ADDRESS_CHARS: usize = 320;
/// A provider's subject, as kept: bounded like the column.
pub const MAX_SUBJECT_CHARS: usize = 255;

/// The account a token proves, at the provider: its immutable subject and
/// the address the provider vouches for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedIdentity {
    pub subject: String,
    pub address: String,
}

/// Why a verified token links no account to this person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRefusal {
    /// The token names no account the provider vouches for (no object ID,
    /// no address the directory or domain verified).
    Unidentified,
    /// The account is another address's than the person's.
    NotTheirs,
}

/// The permissions a token response says were granted, as kept: once
/// each, in order, bounded.
pub fn granted_scopes(scope: Option<&str>) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for scope in scope.unwrap_or_default().split_ascii_whitespace() {
        if !kept.contains(&scope) {
            kept.push(scope);
        }
    }
    let mut text = String::new();
    for scope in kept {
        let next = if text.is_empty() { 0 } else { 1 } + scope.len();
        if text.len() + next > MAX_SCOPES_CHARS {
            break;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(scope);
    }
    text
}

/// The name a link's refresh token is sealed under: it opens only for its
/// company, connector and person.
pub fn sealed_as(connector_id: Uuid, user_id: Uuid) -> String {
    format!("account_link.{connector_id}.{user_id}")
}

#[cfg(test)]
mod google_tests;
#[cfg(test)]
mod tests;
