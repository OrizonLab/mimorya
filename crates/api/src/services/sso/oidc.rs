//! OpenID Connect with a declared provider, through `openidconnect`:
//! discovery, the authorization address (code flow, PKCE, `state`,
//! `nonce`), the code exchange, and the verification of the identity token
//! (signature by the provider's keys, issuer, audience, expiry, nonce).

use mimorya_core::identity_providers::TokenClaims;
use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreAuthenticationFlow, CoreClientAuthMethod,
    CoreErrorResponseType, CoreGenderClaim, CoreJsonWebKey, CoreJweContentEncryptionAlgorithm,
    CoreJwsSigningAlgorithm, CoreProviderMetadata, CoreRevocableToken, CoreRevocationErrorResponse,
    CoreTokenIntrospectionResponse, CoreTokenType,
};
use openidconnect::{
    AdditionalClaims, AuthType, AuthorizationCode, ClaimsVerificationError, ClientId, ClientSecret,
    CsrfToken, EmptyExtraTokenFields, EndpointMaybeSet, EndpointNotSet, EndpointSet, IdTokenClaims,
    IssuerUrl, LoginHint, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl,
    RequestTokenError, Scope, SignatureVerificationError, StandardErrorResponse,
    StandardTokenResponse, TokenResponse,
};
use serde::{Deserialize, Serialize};

use super::http::ProviderHttp;

/// Claims beyond the standard ones that the rules read.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtraClaims {
    /// Microsoft: the account's immutable object ID in its directory.
    #[serde(default)]
    pub oid: Option<String>,
    /// Google: the account's Workspace domain.
    #[serde(default)]
    pub hd: Option<String>,
    /// Microsoft: whether the domain of `email` is verified (a boolean,
    /// read loosely: its form is not documented strictly).
    #[serde(default)]
    pub xms_edov: Option<serde_json::Value>,
}

impl AdditionalClaims for ExtraClaims {}

type IdTokenFields = openidconnect::IdTokenFields<
    ExtraClaims,
    EmptyExtraTokenFields,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJwsSigningAlgorithm,
>;

type OidcClient = openidconnect::Client<
    ExtraClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    StandardTokenResponse<IdTokenFields, CoreTokenType>,
    CoreTokenIntrospectionResponse,
    CoreRevocableToken,
    CoreRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// What went wrong with the provider, for the audit (no token, no code).
pub type Failure = String;

pub type VerifiedClaims = IdTokenClaims<ExtraClaims, CoreGenderClaim>;

/// The provider's declaration of its endpoints and keys, from its issuer,
/// which must be the one it declares.
pub async fn discover(http: &ProviderHttp, issuer: &str) -> Result<CoreProviderMetadata, Failure> {
    let issuer = IssuerUrl::new(issuer.into()).map_err(|e| format!("émetteur invalide : {e}"))?;
    CoreProviderMetadata::discover_async(issuer, http)
        .await
        .map_err(|error| format!("découverte : {}", error_chain(&error)))
}

/// The provider's client for this company's application, authenticating
/// with its secret in the request body when the provider accepts it (some
/// reject the encoded form of a secret in the Basic header).
fn client(
    metadata: CoreProviderMetadata,
    client_id: &str,
    client_secret: Option<&str>,
    redirect_uri: &str,
) -> Result<OidcClient, Failure> {
    let post = metadata
        .token_endpoint_auth_methods_supported()
        .is_some_and(|methods| methods.contains(&CoreClientAuthMethod::ClientSecretPost));
    let redirect =
        RedirectUrl::new(redirect_uri.into()).map_err(|e| format!("adresse de retour : {e}"))?;
    let client = OidcClient::from_provider_metadata(
        metadata,
        ClientId::new(client_id.into()),
        client_secret.map(|s| ClientSecret::new(s.into())),
    )
    .set_redirect_uri(redirect);
    Ok(if post {
        client.set_auth_type(AuthType::RequestBody)
    } else {
        client
    })
}

/// Where the browser signs in, and what the return must match.
pub struct Authorization {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

pub fn authorization(
    metadata: CoreProviderMetadata,
    client_id: &str,
    redirect_uri: &str,
    login_hint: &str,
) -> Result<Authorization, Failure> {
    let client = client(metadata, client_id, None, redirect_uri)?;
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (url, state, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("email".into()))
        .add_scope(Scope::new("profile".into()))
        .set_pkce_challenge(challenge)
        .set_login_hint(LoginHint::new(login_hint.into()))
        .url();
    Ok(Authorization {
        url: url.into(),
        state: state.into_secret(),
        nonce: nonce.secret().clone(),
        pkce_verifier: verifier.into_secret(),
    })
}

/// The company's application at the provider.
pub struct Application<'a> {
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub redirect_uri: &'a str,
}

pub type IdToken = openidconnect::IdToken<
    ExtraClaims,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJwsSigningAlgorithm,
>;

/// The identity token the code is exchanged for, not verified yet.
pub async fn exchange_code(
    http: &ProviderHttp,
    metadata: CoreProviderMetadata,
    application: &Application<'_>,
    code: String,
    pkce_verifier: String,
) -> Result<IdToken, Failure> {
    let client = client(
        metadata,
        application.client_id,
        Some(application.client_secret),
        application.redirect_uri,
    )?;
    let response = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|error| format!("échange : {error}"))?
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
        .request_async(http)
        .await
        .map_err(|error| match error {
            // Of the provider's answer, its error code alone: neither its
            // description nor, on a malformed answer, its body.
            RequestTokenError::ServerResponse(answer) => format!(
                "échange : refusé par le fournisseur ({})",
                error_code(answer.error().as_ref())
            ),
            RequestTokenError::Request(cause) => format!("échange : {cause}"),
            RequestTokenError::Parse(..) => "échange : réponse illisible".into(),
            // The library's own words (empty answer, content type).
            RequestTokenError::Other(text) => format!("échange : {text}"),
        })?;
    response
        .id_token()
        .cloned()
        .ok_or_else(|| "échange : réponse sans jeton d’identité".into())
}

/// Why an identity token was not verified.
pub enum Unverified {
    /// Signed by none of the declaration's keys: perhaps rolled over since
    /// it was read.
    UnknownKey,
    Other(Failure),
}

impl Unverified {
    pub fn failure(self) -> Failure {
        match self {
            Self::UnknownKey => "jeton d’identité : clé de signature inconnue".into(),
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
    let client = client(
        metadata,
        application.client_id,
        Some(application.client_secret),
        application.redirect_uri,
    )
    .map_err(Unverified::Other)?;
    token
        .claims(&client.id_token_verifier(), &Nonce::new(nonce.into()))
        .cloned()
        .map_err(|error| match error {
            ClaimsVerificationError::SignatureVerification(
                SignatureVerificationError::NoMatchingKey,
            ) => Unverified::UnknownKey,
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
        email_domain_verified: extra.xms_edov.as_ref().map(|value| {
            matches!(value, serde_json::Value::Bool(true))
                || value.as_str() == Some("true")
                || value.as_u64() == Some(1)
                || value.as_str() == Some("1")
        }),
    }
}

/// A provider's error code (`access_denied`…), as it may be audited: letters,
/// digits and underscores, bounded.
pub fn error_code(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(64)
        .collect()
}

/// An error and its causes, on one line, bounded: a discovery reads only
/// public documents.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(" : ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text.chars().take(300).collect()
}
