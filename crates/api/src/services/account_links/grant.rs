//! The delegated grant redeemed (ADR-116): the code is exchanged for an
//! identity token and a refresh token, with the permissions the person
//! granted. The access token the code also brings is not kept: a run asks
//! for its own.

use mimorya_core::account_links::granted_scopes;
use openidconnect::core::CoreProviderMetadata;
use openidconnect::{OAuth2TokenResponse, TokenResponse};

use crate::services::sso::{Application, Failure, IdToken, ProviderHttp, redeem};

/// What the code is redeemed for: the identity token, not verified yet,
/// the refresh token and the permissions the person granted.
pub struct Granted {
    pub id_token: IdToken,
    pub refresh_token: String,
    pub scopes: String,
}

pub async fn redeem_grant(
    http: &ProviderHttp,
    metadata: CoreProviderMetadata,
    application: &Application<'_>,
    code: String,
    pkce_verifier: String,
) -> Result<Granted, Failure> {
    let tokens = redeem(http, metadata, application, code, pkce_verifier).await?;
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
