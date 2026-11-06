//! An identity token a provider gave, verified (ADR-063): its signature by
//! the keys of the provider's declaration, its issuer, its audience (the
//! client it was asked for), its expiry and the attempt's nonce; then what
//! the rules read of its claims. A sign-in and a link verify one for a
//! company's application, which has a secret; an MCP server's authorization
//! for its declared client, with a secret or without (ADR-175).

use mimorya_core::identity_providers::TokenClaims;
use openidconnect::core::CoreProviderMetadata;
use openidconnect::{
    ClaimsVerificationError, ClientId, ClientSecret, Nonce, SignatureVerificationError,
};

use super::oidc::{Application, Failure, IdToken, OidcClient, VerifiedClaims};

/// Why an identity token was not verified.
pub enum Unverified {
    /// Its signature verified by none of the declaration's keys: perhaps
    /// rolled over since they were read, with or without a key identifier.
    Signature(SignatureVerificationError),
    Other(Failure),
}

impl Unverified {
    pub fn failure(self) -> Failure {
        match self {
            Self::Signature(SignatureVerificationError::NoMatchingKey) => {
                "jeton d’identité : clé de signature inconnue".into()
            }
            Self::Signature(error) => format!("jeton d’identité : signature refusée ({error})"),
            Self::Other(failure) => failure,
        }
    }
}

/// The token's claims, verified against the declaration's keys, issuer,
/// the application's audience, its expiry and the attempt's nonce.
pub fn verify(
    metadata: CoreProviderMetadata,
    application: &Application<'_>,
    token: &IdToken,
    nonce: &str,
) -> Result<VerifiedClaims, Unverified> {
    let secret = Some(application.client_secret);
    verify_for(metadata, application.client_id, secret, token, nonce)
}

/// The same for the client `client_id`, with its secret if it has one: a
/// token signed with a secret is never taken for a client without one.
pub fn verify_for(
    metadata: CoreProviderMetadata,
    client_id: &str,
    client_secret: Option<&str>,
    token: &IdToken,
    nonce: &str,
) -> Result<VerifiedClaims, Unverified> {
    let client = OidcClient::from_provider_metadata(
        metadata,
        ClientId::new(client_id.into()),
        client_secret.map(|secret| ClientSecret::new(secret.into())),
    );
    token
        .claims(&client.id_token_verifier(), &Nonce::new(nonce.into()))
        .cloned()
        .map_err(|error| match error {
            ClaimsVerificationError::SignatureVerification(error) => Unverified::Signature(error),
            error => Unverified::Other(format!("jeton d’identité : {error}")),
        })
}

/// What the rules read of verified claims.
pub fn token_claims(claims: &VerifiedClaims) -> TokenClaims<'_> {
    let extra = claims.additional_claims();
    TokenClaims {
        subject: claims.subject().as_str(),
        email: claims.email().map(|e| e.as_str()),
        email_verified: claims.email_verified(),
        preferred_username: claims.preferred_username().map(|u| u.as_str()),
        oid: extra.oid.as_deref(),
        hosted_domain: extra.hd.as_deref(),
        name: claims.name().and_then(|n| n.get(None)).map(|n| n.as_str()),
        given_name: claims
            .given_name()
            .and_then(|n| n.get(None))
            .map(|n| n.as_str()),
        family_name: claims
            .family_name()
            .and_then(|n| n.get(None))
            .map(|n| n.as_str()),
        email_domain_verified: extra.xms_edov.as_ref().map(|value| {
            matches!(value, serde_json::Value::Bool(true))
                || value.as_str() == Some("true")
                || value.as_u64() == Some(1)
                || value.as_str() == Some("1")
        }),
    }
}
