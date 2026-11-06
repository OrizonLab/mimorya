//! The account a person authorizes an MCP server with, at its authorization
//! server (ADR-175): when the declaration asks for it, that server must
//! prove, by an identity token the platform verified, that the account is
//! the person's own. The address it vouches for is read as a sign-in reads
//! it (ADR-063, ADR-067): Microsoft and Google, whose issuers are known, by
//! their own rules (the directory's sign-in name; an address Google
//! verified in the account's Workspace domain), which `email_verified`
//! alone would not satisfy; any other issuer by an address it says it
//! verified. That address must be the person's, but for case.

use super::{LinkKind, LinkRefusal, LinkedIdentity, MAX_ADDRESS_CHARS, MAX_SUBJECT_CHARS};
use crate::identity_providers::{ProviderKind, TokenClaims, issuer_kind};

/// The scopes asked besides those declared, for the identity token and
/// what the rules read of it: the address, and the sign-in name and object
/// ID a Microsoft directory gives only with `profile`.
pub const IDENTITY_SCOPES: [&str; 3] = ["openid", "email", "profile"];

/// The account the authorization server of `issuer` proves, by verified
/// `claims`, the person authorized with, if it is the person's own.
pub fn authorizing_identity(
    issuer: &str,
    claims: &TokenClaims<'_>,
    account_email: &str,
) -> Result<LinkedIdentity, LinkRefusal> {
    match issuer_kind(issuer) {
        Some(ProviderKind::Microsoft) => {
            LinkKind::Microsoft365.linked_identity(claims, account_email)
        }
        Some(ProviderKind::Google) => {
            LinkKind::GoogleWorkspace.linked_identity(claims, account_email)
        }
        _ => verified_address(claims, account_email),
    }
}

/// Any other issuer: its subject, and an address it says it verified.
fn verified_address(
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
    if !address.eq_ignore_ascii_case(account_email.trim()) {
        return Err(LinkRefusal::NotTheirs);
    }
    Ok(LinkedIdentity {
        subject: subject.into(),
        address: address.to_ascii_lowercase(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity_providers::GOOGLE_ISSUER;

    const ISSUER: &str = "https://auth.example.com";
    const MARC: &str = "marc.durand@neo-maintenance.example";

    fn claims<'a>(email: &'a str, verified: Option<bool>) -> TokenClaims<'a> {
        TokenClaims {
            subject: "sujet-42",
            email: Some(email),
            email_verified: verified,
            ..TokenClaims::default()
        }
    }

    #[test]
    fn an_address_the_server_verified_proves_the_person_s_own_account() {
        let proven = authorizing_identity(
            ISSUER,
            &claims("Marc.Durand@Neo-Maintenance.example", Some(true)),
            MARC,
        )
        .expect("theirs");
        assert_eq!(proven.subject, "sujet-42");
        assert_eq!(proven.address, MARC);
    }

    #[test]
    fn an_address_not_verified_proves_nothing() {
        for verified in [Some(false), None] {
            assert_eq!(
                authorizing_identity(ISSUER, &claims(MARC, verified), MARC),
                Err(LinkRefusal::Unidentified),
                "{verified:?}"
            );
        }
        let none = TokenClaims {
            subject: "sujet-42",
            email_verified: Some(true),
            ..TokenClaims::default()
        };
        assert_eq!(
            authorizing_identity(ISSUER, &none, MARC),
            Err(LinkRefusal::Unidentified)
        );
        let anonymous = TokenClaims {
            subject: "",
            ..claims(MARC, Some(true))
        };
        assert_eq!(
            authorizing_identity(ISSUER, &anonymous, MARC),
            Err(LinkRefusal::Unidentified)
        );
    }

    #[test]
    fn another_address_is_not_the_person_s_account() {
        let paul = "paul.martin@neo-maintenance.example";
        assert_eq!(
            authorizing_identity(ISSUER, &claims(paul, Some(true)), MARC),
            Err(LinkRefusal::NotTheirs)
        );
    }

    #[test]
    fn microsoft_and_google_issuers_follow_their_own_rules() {
        // `email_verified` alone is not what either vouches by.
        let verified = claims(MARC, Some(true));
        let microsoft =
            "https://login.microsoftonline.com/0f3c1a52-0000-4000-8000-000000000001/v2.0";
        assert_eq!(
            authorizing_identity(microsoft, &verified, MARC),
            Err(LinkRefusal::Unidentified)
        );
        let directory = TokenClaims {
            oid: Some("objet-7"),
            preferred_username: Some(MARC),
            ..TokenClaims::default()
        };
        let proven = authorizing_identity(microsoft, &directory, MARC).expect("theirs");
        assert_eq!(proven.subject, "objet-7");
        // A consumer Google account bearing the address is not theirs.
        assert_eq!(
            authorizing_identity("https://accounts.google.com/", &verified, MARC),
            Err(LinkRefusal::Unidentified)
        );
        let workspace = TokenClaims {
            hosted_domain: Some("neo-maintenance.example"),
            ..claims(MARC, Some(true))
        };
        assert!(authorizing_identity(GOOGLE_ISSUER, &workspace, MARC).is_ok());
    }
}
