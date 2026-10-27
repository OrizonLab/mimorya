//! A company's Microsoft 365 application (ADR-116): its directory and
//! client IDs, the delegated Graph permissions its people grant (reading,
//! and writing when the company lets its tools write, ADR-119), and the
//! directory account a token must prove to be the person's own.

use uuid::Uuid;

use super::{LinkRefusal, LinkedIdentity, MAX_ADDRESS_CHARS, MAX_SUBJECT_CHARS};
use crate::identity_providers::{TokenClaims, client_secret_problem, microsoft_issuer};

/// The connector type of a company's Microsoft 365 application.
pub const MICROSOFT365: &str = "microsoft365";
/// Microsoft Graph, which the tools call with a person's token.
pub const MICROSOFT_GRAPH: &str = "https://graph.microsoft.com/v1.0";
/// Graph's delegated permissions the tools need (ADR-117, ADR-119).
pub const MAIL_READ: &str = "Mail.Read";
pub const CALENDARS_READ: &str = "Calendars.Read";
pub const MAIL_SEND: &str = "Mail.Send";
pub const CALENDARS_WRITE: &str = "Calendars.ReadWrite";
/// What a person grants, besides `openid`: their address, the right to be
/// reached while away (a refresh token), reading their mail and calendar.
pub const MICROSOFT365_SCOPES: [&str; 5] = [
    "email",
    "profile",
    "offline_access",
    MAIL_READ,
    CALENDARS_READ,
];
/// What a person grants besides when their company lets its tools write
/// (ADR-119): sending mail as them, writing their calendar.
pub const MICROSOFT365_WRITE_SCOPES: [&str; 2] = [MAIL_SEND, CALENDARS_WRITE];
/// The permission that brings a refresh token: always renewed.
pub(super) const OFFLINE_ACCESS: &str = "offline_access";

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

/// The Microsoft account a verified token proves, if it is the person's
/// own: the directory's object ID, and the address the directory vouches
/// for, as a sign-in reads it (`proven_identity`): its sign-in name, whose
/// domain it verified, else an address whose domain it verified. That
/// address must be the person's, but for case. Another account, even one
/// the person may open, is never linked to theirs.
pub(super) fn linked_identity(
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
