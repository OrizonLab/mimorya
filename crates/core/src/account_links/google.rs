//! A company's Google Workspace application (ADR-121): its OAuth client, the
//! delegated Gmail and Calendar permissions its people grant (reading, and
//! writing when the company lets its tools write), and the Workspace
//! account a token must prove to be the person's own: an address Google
//! verified, in the Workspace domain Google names for the account.

use super::{LinkRefusal, LinkedIdentity, MAX_ADDRESS_CHARS, MAX_SUBJECT_CHARS};
use crate::identity_providers::{GOOGLE_ISSUER, TokenClaims, client_secret_problem};

/// The connector type of a company's Google Workspace application.
pub const GOOGLE_WORKSPACE: &str = "google_workspace";
/// The Gmail and Calendar APIs, which the tools call with a person's token.
pub const GOOGLE_GMAIL: &str = "https://gmail.googleapis.com/gmail/v1";
pub const GOOGLE_CALENDAR: &str = "https://www.googleapis.com/calendar/v3";
/// The delegated permissions the tools need.
pub const GMAIL_READONLY: &str = "https://www.googleapis.com/auth/gmail.readonly";
pub const CALENDAR_READONLY: &str = "https://www.googleapis.com/auth/calendar.readonly";
pub const GMAIL_SEND: &str = "https://www.googleapis.com/auth/gmail.send";
pub const CALENDAR_EVENTS: &str = "https://www.googleapis.com/auth/calendar.events";
/// What an OAuth client ID of Google Cloud ends with.
const CLIENT_ID_SUFFIX: &str = ".apps.googleusercontent.com";
const CLIENT_ID_MAX_CHARS: usize = 255;

/// A company's Google Workspace application, once checked: where its people
/// sign in to grant access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleWorkspaceApplication {
    pub client_id: String,
    pub issuer: String,
}

/// The application a declaration designates, or why it is refused (in
/// French): an OAuth client ID as Google Cloud shows it, and its secret.
pub fn google_workspace_application(
    client_id: &str,
    client_secret: &str,
) -> Result<GoogleWorkspaceApplication, String> {
    let client_id = client_id.trim();
    let well_formed = client_id.len() <= CLIENT_ID_MAX_CHARS
        && client_id
            .strip_suffix(CLIENT_ID_SUFFIX)
            .is_some_and(|start| {
                !start.is_empty()
                    && start
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
            });
    if !well_formed {
        return Err(format!(
            "L’identifiant du client OAuth est celui que Google Cloud montre, \
             terminé par {CLIENT_ID_SUFFIX}"
        ));
    }
    if let Some(problem) = client_secret_problem(client_secret) {
        return Err(problem);
    }
    Ok(GoogleWorkspaceApplication {
        client_id: client_id.into(),
        issuer: GOOGLE_ISSUER.into(),
    })
}

/// The Workspace account a verified token proves, if it is the person's
/// own: Google's subject, and an address Google verified whose domain is
/// the account's Workspace domain (`hd`), so that the domain's
/// administrators, not anyone who registered that address at Google,
/// vouch for it. That address must be the person's, but for case. A
/// consumer Google account, even one bearing their address, is never
/// linked to theirs.
pub(super) fn linked_identity(
    claims: &TokenClaims<'_>,
    account_email: &str,
) -> Result<LinkedIdentity, LinkRefusal> {
    let subject = Some(claims.subject)
        .filter(|subject| (1..=MAX_SUBJECT_CHARS).contains(&subject.len()))
        .ok_or(LinkRefusal::Unidentified)?;
    let address = claims
        .email
        .filter(|_| claims.email_verified == Some(true))
        .filter(|address| address.len() <= MAX_ADDRESS_CHARS)
        .ok_or(LinkRefusal::Unidentified)?;
    let in_workspace = address.rsplit_once('@').is_some_and(|(_, domain)| {
        claims
            .hosted_domain
            .is_some_and(|hosted| hosted.eq_ignore_ascii_case(domain))
    });
    if !in_workspace {
        return Err(LinkRefusal::Unidentified);
    }
    if !address.eq_ignore_ascii_case(account_email.trim()) {
        return Err(LinkRefusal::NotTheirs);
    }
    Ok(LinkedIdentity {
        subject: subject.into(),
        address: address.to_ascii_lowercase(),
    })
}

/// The domain Google is asked to offer accounts of (`hd`): the person's
/// address's.
pub(super) fn hosted_domain_hint(account_email: &str) -> Option<String> {
    account_email
        .trim()
        .rsplit_once('@')
        .map(|(_, domain)| domain.to_ascii_lowercase())
        .filter(|domain| !domain.is_empty())
}
