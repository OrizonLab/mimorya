//! The delegated grant at the provider (ADR-116): where the browser goes
//! to sign in and grant the connector's permissions (code flow, PKCE,
//! `state`, `nonce`, the person's address as a hint), and the code redeemed
//! for an identity token and a refresh token. The access token the code
//! also brings is not kept: a run asks for its own.

use mimorya_core::account_links::granted_scopes;
use openidconnect::core::{CoreAuthenticationFlow, CoreProviderMetadata};
use openidconnect::{
    CsrfToken, LoginHint, Nonce, OAuth2TokenResponse, PkceCodeChallenge, Scope, TokenResponse,
};

use crate::services::sso::http::ProviderHttp;
use crate::services::sso::oidc::{self, Application, Failure, IdToken};

/// Where the browser signs in, and what the return must match.
pub struct Departure {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

pub fn departure(
    metadata: CoreProviderMetadata,
    client_id: &str,
    redirect_uri: &str,
    login_hint: &str,
    scopes: &[&str],
) -> Result<Departure, Failure> {
    let client = oidc::client(metadata, client_id, None, redirect_uri)?;
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let mut request = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .set_pkce_challenge(challenge)
        .set_login_hint(LoginHint::new(login_hint.into()));
    for scope in scopes {
        request = request.add_scope(Scope::new((*scope).into()));
    }
    let (url, state, nonce) = request.url();
    Ok(Departure {
        url: url.into(),
        state: state.into_secret(),
        nonce: nonce.secret().clone(),
        pkce_verifier: verifier.into_secret(),
    })
}

/// What the code is redeemed for: the identity token, not verified yet,
/// the refresh token and the permissions the person granted.
pub struct Granted {
    pub id_token: IdToken,
    pub refresh_token: String,
    pub scopes: String,
}

pub async fn redeem(
    http: &ProviderHttp,
    metadata: CoreProviderMetadata,
    application: &Application<'_>,
    code: String,
    pkce_verifier: String,
) -> Result<Granted, Failure> {
    let tokens = oidc::redeem(http, metadata, application, code, pkce_verifier).await?;
    let id_token = tokens
        .id_token()
        .cloned()
        .ok_or("échange : réponse sans jeton d’identité")?;
    // Without it, nothing could be read while the person is away.
    let refresh_token = tokens
        .refresh_token()
        .map(|token| token.secret().clone())
        .filter(|token| !token.is_empty())
        .ok_or("échange : accès hors connexion non accordé (pas de jeton de renouvellement)")?;
    let scopes = tokens.scopes().map(|scopes| {
        scopes
            .iter()
            .map(|scope| scope.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    });
    Ok(Granted {
        id_token,
        refresh_token,
        scopes: granted_scopes(scopes.as_deref()),
    })
}
