//! Accounts a person links at a provider their company declared (ADR-116):
//! what a company declares of its Microsoft 365 application, the delegated
//! permissions a person grants (never an application permission, which
//! would open every mailbox of the directory), and when a verified token
//! proves the linked account is the person's own.

use uuid::Uuid;

use crate::identity_providers::{TokenClaims, client_secret_problem, microsoft_issuer};

/// The connector type of a company's Microsoft 365 application.
pub const MICROSOFT365: &str = "microsoft365";
/// Microsoft Graph, which the tools call with a person's token.
pub const MICROSOFT_GRAPH: &str = "https://graph.microsoft.com/v1.0";
/// What a person grants, besides `openid`: their address, the right to be
/// reached while away (a refresh token), reading their mail and calendar.
pub const MICROSOFT365_SCOPES: [&str; 5] = [
    "email",
    "profile",
    "offline_access",
    "Mail.Read",
    "Calendars.Read",
];
/// The permission that brings a refresh token.
const OFFLINE_ACCESS: &str = "offline_access";
/// The permissions granted, as kept: bounded like the column.
pub const MAX_SCOPES_CHARS: usize = 1000;
/// A provider's address, as kept: bounded like the column.
pub const MAX_ADDRESS_CHARS: usize = 320;
/// A provider's subject, as kept: bounded like the column.
pub const MAX_SUBJECT_CHARS: usize = 255;

/// A company's Microsoft 365 application, once checked: where its people
/// sign in to grant access (its directory's issuer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Microsoft365Application {
    pub directory: Uuid,
    pub client_id: Uuid,
    pub issuer: String,
}

/// The application a declaration designates, or why it is refused (in
/// French): the directory (tenant ID) and application (client) ID are the
/// GUIDs Microsoft Entra shows, the secret one its portal gave.
pub fn microsoft365_application(
    directory: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<Microsoft365Application, String> {
    let guid = |text: &str, what: &str| {
        Uuid::parse_str(text.trim())
            .map_err(|_| format!("{what} est un GUID (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)"))
    };
    let directory = guid(directory, "L’identifiant de l’annuaire (ID de locataire)")?;
    let client_id = guid(client_id, "L’identifiant de l’application (ID client)")?;
    if let Some(problem) = client_secret_problem(client_secret) {
        return Err(problem);
    }
    Ok(Microsoft365Application {
        directory,
        client_id,
        issuer: microsoft_issuer(directory),
    })
}

/// The account a token proves, at the provider: its immutable subject and
/// the address the directory vouches for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedIdentity {
    pub subject: String,
    pub address: String,
}

/// Why a verified token links no account to this person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRefusal {
    /// The token names no account of the directory (no object ID, no
    /// address the directory verified).
    Unidentified,
    /// The account is another address's than the person's.
    NotTheirs,
}

/// The Microsoft account a verified token proves, if it is the person's
/// own: the directory's object ID, and the address the directory vouches
/// for, as a sign-in reads it (`proven_identity`): its sign-in name, whose
/// domain it verified, else an address whose domain it verified. That
/// address must be the person's, but for case. Another account, even one
/// the person may open, is never linked to theirs.
pub fn linked_identity(
    claims: &TokenClaims<'_>,
    account_email: &str,
) -> Result<LinkedIdentity, LinkRefusal> {
    let subject = claims
        .oid
        .filter(|oid| (1..=MAX_SUBJECT_CHARS).contains(&oid.len()))
        .ok_or(LinkRefusal::Unidentified)?;
    let address = claims
        .preferred_username
        .filter(|name| name.contains('@'))
        .or(claims
            .email
            .filter(|_| claims.email_domain_verified == Some(true)))
        .filter(|address| address.len() <= MAX_ADDRESS_CHARS)
        .ok_or(LinkRefusal::Unidentified)?;
    if !address.eq_ignore_ascii_case(account_email.trim()) {
        return Err(LinkRefusal::NotTheirs);
    }
    Ok(LinkedIdentity {
        subject: subject.into(),
        address: address.to_ascii_lowercase(),
    })
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

/// The permissions a link's refresh token is renewed with (ADR-117): the
/// platform's, among those the person granted (the provider renews no
/// more, and a permission the platform added since would be refused),
/// compared on their last segment (Microsoft may name a Graph permission
/// by its full address); offline access always, which providers do not
/// always list; all of them when the provider did not say what it granted.
pub fn renewal_scopes(granted: &str) -> Vec<&'static str> {
    let granted: Vec<&str> = granted
        .split_ascii_whitespace()
        .map(|scope| scope.rsplit('/').next().unwrap_or(scope))
        .collect();
    if granted.is_empty() {
        return MICROSOFT365_SCOPES.to_vec();
    }
    MICROSOFT365_SCOPES
        .into_iter()
        .filter(|scope| {
            *scope == OFFLINE_ACCESS
                || granted
                    .iter()
                    .any(|granted| granted.eq_ignore_ascii_case(scope))
        })
        .collect()
}

/// The name a link's refresh token is sealed under: it opens only for its
/// company, connector and person.
pub fn sealed_as(connector_id: Uuid, user_id: Uuid) -> String {
    format!("account_link.{connector_id}.{user_id}")
}

#[cfg(test)]
mod tests;
