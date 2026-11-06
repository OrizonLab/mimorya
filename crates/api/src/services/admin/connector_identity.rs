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

    #[test]
    fn a_declaration_written_again_keeps_the_verification_while_each_person_authorizes() {
        let verifying: OAuthDeclaration = serde_json::from_value(serde_json::json!({
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
        .expect("declaration");
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
}
