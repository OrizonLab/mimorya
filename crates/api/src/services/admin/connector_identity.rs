//! Whether an MCP server's declaration verifies the identity of each person
//! who authorizes it (ADR-175), and what that asks of its authorization
//! server: only a server each person authorizes may (the company's
//! authorization is an administrator's, often a service account's); the
//! scopes of the identity token are asked with those declared, the client
//! registered for them; and the server must read as an OpenID provider,
//! its configuration and keys, before anything is declared.

use mimorya_connectors::mcp::oauth::{
    Discovery, OAuthDeclaration, OAuthHolder, asked_scopes, with_identity_scopes,
};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_core::DomainError;

use crate::services::sso::{ProviderHttp, discover};

/// Why the company's authorization is never verified so.
const COMPANY_HOLDER: &str = "Seule l’autorisation que chaque personne donne pour elle-même \
     peut vérifier son identité";

/// Whether the declaration verifies identities: as asked, else as the one
/// it replaces did while each person authorizes the server; never for the
/// company's authorization, which `holder` would be.
pub(super) fn verifies_identity(
    asked: Option<bool>,
    holder: OAuthHolder,
    current: Option<&OAuthDeclaration>,
) -> Result<bool, DomainError> {
    match (asked, holder) {
        (Some(true), OAuthHolder::Company) => Err(DomainError::validation(COMPANY_HOLDER)),
        (Some(asked), _) => Ok(asked),
        (None, OAuthHolder::Company) => Ok(false),
        (None, OAuthHolder::Person) => Ok(current.is_some_and(|d| d.verifies_identity)),
    }
}

/// Why a declaration that took long to find its server starts again.
const CHANGED_MEANWHILE: &str = "Qui autorise ce serveur, ou la vérification de l’identité, a \
     changé pendant sa déclaration : recommencez";

/// Whether `verify`, decided on the declaration read before the discovery
/// (`read`), still holds under the lock: refused, to start again, if each
/// person no longer authorizes the server, or if the declaration, kept as
/// the one replaced said (`asked` unsaid), changed its verification
/// meanwhile, another administrator's change never undone unseen.
pub(super) fn verification_unchanged(
    verify: bool,
    asked: Option<bool>,
    read: bool,
    holder: OAuthHolder,
    current: Option<&OAuthDeclaration>,
) -> Result<(), DomainError> {
    let now = current.is_some_and(|declared| declared.verifies_identity);
    if (verify && holder != OAuthHolder::Person) || (asked.is_none() && now != read) {
        return Err(DomainError::conflict(CHANGED_MEANWHILE));
    }
    Ok(())
}

/// The scopes to ask for, those of the identity token added, once the
/// authorization server `found` reads as an OpenID provider.
pub(super) async fn identity_scopes(
    found: &Discovery,
    scopes: Option<Vec<String>>,
    network: NetworkPolicy,
) -> Result<Vec<String>, String> {
    let scopes = with_identity_scopes(asked_scopes(found, scopes)?)?;
    let http = ProviderHttp::new(network).map_err(|error| error.to_string())?;
    discover(&http, &found.issuer).await.map_err(|failure| {
        format!(
            "Le serveur d’autorisation ne se déclare pas fournisseur OpenID Connect : l’identité \
             des personnes n’y peut être vérifiée ({failure})"
        )
    })?;
    Ok(scopes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_each_person_s_authorization_verifies_identities() {
        let person = OAuthHolder::Person;
        let company = OAuthHolder::Company;
        assert!(verifies_identity(Some(true), company, None).is_err());
        assert_eq!(verifies_identity(Some(true), person, None).ok(), Some(true));
        assert_eq!(
            verifies_identity(Some(false), person, None).ok(),
            Some(false)
        );
        assert_eq!(verifies_identity(None, person, None).ok(), Some(false));
        assert_eq!(verifies_identity(None, company, None).ok(), Some(false));
    }

    /// A declaration each person authorizes, verifying who does.
    fn verifying() -> OAuthDeclaration {
        serde_json::from_value(serde_json::json!({
            "issuer": "https://as.example",
            "resource": "https://mcp.example/mcp",
            "authorizationEndpoint": "https://as.example/authorize",
            "tokenEndpoint": "https://as.example/token",
            "namesIssuerOnReturn": false,
            "clientId": "mimorya",
            "clientAuth": "none",
            "scopes": ["openid", "email", "profile"],
            "holder": "person",
            "verifiesIdentity": true,
        }))
        .expect("declaration")
    }

    #[test]
    fn a_declaration_written_again_keeps_the_verification_while_each_person_authorizes() {
        let verifying = verifying();
        let current = Some(&verifying);
        assert_eq!(
            verifies_identity(None, OAuthHolder::Person, current).ok(),
            Some(true)
        );
        assert_eq!(
            verifies_identity(Some(false), OAuthHolder::Person, current).ok(),
            Some(false)
        );
        // Moved to the company: dropped, never refused for being kept.
        assert_eq!(
            verifies_identity(None, OAuthHolder::Company, current).ok(),
            Some(false)
        );
    }

    #[test]
    fn a_verification_changed_meanwhile_starts_the_declaration_again() {
        let (person, company) = (OAuthHolder::Person, OAuthHolder::Company);
        let verifying = verifying();
        // Nothing changed: kept, asked or not.
        assert!(verification_unchanged(true, None, true, person, Some(&verifying)).is_ok());
        assert!(verification_unchanged(false, Some(false), true, person, Some(&verifying)).is_ok());
        // Turned on by another while this one, unsaid, read it off.
        assert!(verification_unchanged(false, None, false, person, Some(&verifying)).is_err());
        // Turned off meanwhile while this one kept it on.
        assert!(verification_unchanged(true, None, true, person, None).is_err());
        // The company now authorizes the server.
        assert!(verification_unchanged(true, Some(true), false, company, None).is_err());
        assert!(verification_unchanged(false, Some(false), false, company, None).is_ok());
    }
}
