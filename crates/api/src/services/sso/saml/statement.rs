//! What a verified SAML assertion says (ADR-068), read for the rules an
//! OpenID Connect token follows: its subject's NameID, and the attributes
//! the usual providers send, under their Microsoft, LDAP or plain names.
//! A transient NameID changes at each sign-in: it serves only beside a
//! stable object ID.

use mimorya_core::identity_providers::TokenClaims;
use saml_rs::{NameIdFormat, SsoSession};

const OBJECT_ID: &[&str] = &["http://schemas.microsoft.com/identity/claims/objectidentifier"];
const EMAIL: &[&str] = &[
    "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress",
    "urn:oid:0.9.2342.19200300.100.1.3",
    "email",
    "mail",
];
const DISPLAY_NAME: &[&str] = &[
    "http://schemas.microsoft.com/identity/claims/displayname",
    "urn:oid:2.16.840.1.113730.3.1.241",
    "displayName",
];
const GIVEN_NAME: &[&str] = &[
    "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/givenname",
    "urn:oid:2.5.4.42",
    "givenName",
];
const SURNAME: &[&str] = &[
    "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/surname",
    "urn:oid:2.5.4.4",
    "sn",
    "surname",
];

/// The assertion's subject and attributes, owned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Statement {
    pub name_id: String,
    pub transient: bool,
    pub object_id: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub given_name: Option<String>,
    pub surname: Option<String>,
}

impl Statement {
    pub fn of(session: &SsoSession) -> Self {
        let first = |names: &[&str]| {
            names.iter().find_map(|name| {
                session
                    .attributes()
                    .get(name)
                    .and_then(|attribute| attribute.values().first())
                    .map(|value| value.as_str().trim().to_string())
                    .filter(|value| !value.is_empty())
            })
        };
        let name_id = session.name_id();
        Self {
            name_id: name_id.value().trim().to_string(),
            transient: matches!(name_id.format(), Some(NameIdFormat::Transient)),
            object_id: first(OBJECT_ID),
            email: first(EMAIL),
            display_name: first(DISPLAY_NAME),
            given_name: first(GIVEN_NAME),
            surname: first(SURNAME),
        }
    }

    /// For the rules (`proven_identity`): refused when nothing identifies
    /// the person from one sign-in to the next.
    pub fn claims(&self) -> Result<TokenClaims<'_>, &'static str> {
        if self.transient && self.object_id.is_none() {
            return Err("SAML : identifiant transitoire, sans identifiant stable de la personne");
        }
        // A transient NameID names no one for good: neither subject nor
        // address.
        let name_id = (!self.transient).then_some(self.name_id.as_str());
        Ok(TokenClaims {
            subject: name_id.unwrap_or_default(),
            preferred_username: name_id,
            email: self.email.as_deref(),
            oid: self.object_id.as_deref(),
            name: self.display_name.as_deref(),
            given_name: self.given_name.as_deref(),
            family_name: self.surname.as_deref(),
            ..TokenClaims::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entra() -> Statement {
        Statement {
            name_id: "Nadia.Benali@neo-maintenance.example".into(),
            transient: false,
            object_id: Some("00000000-0000-0000-0000-0000000000aa".into()),
            email: Some("nadia@neo-maintenance.example".into()),
            display_name: Some("Nadia Benali".into()),
            given_name: Some("Nadia".into()),
            surname: Some("Benali".into()),
        }
    }

    #[test]
    fn its_nameid_and_attributes_become_the_claims() {
        let statement = entra();
        let claims = statement.claims().unwrap();
        assert_eq!(claims.subject, "Nadia.Benali@neo-maintenance.example");
        assert_eq!(claims.preferred_username, Some(claims.subject));
        assert_eq!(claims.oid, Some("00000000-0000-0000-0000-0000000000aa"));
        assert_eq!(claims.email, Some("nadia@neo-maintenance.example"));
        assert_eq!(claims.name, Some("Nadia Benali"));
        assert_eq!(claims.given_name, Some("Nadia"));
        assert_eq!(claims.family_name, Some("Benali"));
        // No verification flag: the provider signed the assertion.
        assert_eq!(claims.email_verified, None);
    }

    #[test]
    fn a_transient_nameid_names_no_one() {
        let transient = Statement {
            transient: true,
            ..entra()
        };
        let claims = transient.claims().unwrap();
        assert_eq!(claims.subject, "");
        assert_eq!(claims.preferred_username, None);
        assert_eq!(claims.oid, Some("00000000-0000-0000-0000-0000000000aa"));
        let alone = Statement {
            object_id: None,
            ..transient
        };
        assert!(alone.claims().is_err());
    }
}
