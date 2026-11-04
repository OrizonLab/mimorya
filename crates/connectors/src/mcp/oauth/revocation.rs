//! The tokens of an authorization the platform lets go of, revoked at its
//! authorization server (RFC 7009, ADR-172): at the revocation endpoint the
//! declaration keeps, the client proving itself as at the token endpoint;
//! the refresh token first (most servers take its whole grant with it),
//! then the access token. A server that names no revocation endpoint, does
//! not answer or refuses is said, never by what it wrote around its error
//! code, and never with a token.

use super::declaration::OAuthDeclaration;
use super::token_request::{FormEndpoint, TokenAnswer, post_form};
use crate::outbound::NetworkPolicy;

/// What is said when the server names no revocation endpoint.
pub const NO_REVOCATION_ENDPOINT: &str = "Le serveur d’autorisation n’annonce pas de point de \
     révocation : retirez l’accord chez lui";

/// A token to revoke, and what it is (`token_type_hint`).
pub struct HeldToken<'a> {
    pub token: &'a str,
    pub refresh: bool,
}

/// Asks the declared revocation endpoint to revoke `tokens`, in order; the
/// first that is not revoked stops and says why.
pub async fn revoke_tokens(
    declaration: &OAuthDeclaration,
    tokens: &[HeldToken<'_>],
    client_secret: Option<&str>,
    policy: NetworkPolicy,
) -> Result<(), String> {
    let Some(url) = declaration.revocation_endpoint.as_deref() else {
        return Err(NO_REVOCATION_ENDPOINT.into());
    };
    let endpoint = FormEndpoint {
        url,
        named: "Point de révocation",
        asked: "la demande de révocation",
    };
    for held in tokens {
        let hint = if held.refresh {
            "refresh_token"
        } else {
            "access_token"
        };
        let params = [("token", held.token), ("token_type_hint", hint)];
        match post_form(declaration, &endpoint, &params, client_secret, policy).await {
            TokenAnswer::Granted(_) => {}
            TokenAnswer::Refused { said, .. } | TokenAnswer::Failed(said) => return Err(said),
        }
    }
    Ok(())
}
