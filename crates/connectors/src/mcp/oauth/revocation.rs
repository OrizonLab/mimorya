//! The tokens of an authorization the platform lets go of, revoked at its
//! authorization server (RFC 7009, ADR-172): at the revocation endpoint the
//! declaration keeps, the client proving itself as at the token endpoint;
//! the refresh token first (most servers take its whole grant with it),
//! then the access token, which a server may not revoke once the refresh
//! token was. A declaration that knows no revocation endpoint, a server
//! that does not answer or refuses is said, never by what it wrote around
//! its error code, and never with a token.

use super::declaration::OAuthDeclaration;
use super::token_request::{FormEndpoint, TokenAnswer, post_form};
use crate::outbound::NetworkPolicy;

/// What is said when the declaration knows no revocation endpoint: the
/// server named none when it was declared.
pub const NO_REVOCATION_ENDPOINT: &str = "Aucun point de révocation n’est connu pour ce serveur \
     d’autorisation : retirez l’accord chez lui";

/// A token to revoke, and what it is (`token_type_hint`).
pub struct HeldToken<'a> {
    pub token: &'a str,
    pub refresh: bool,
}

/// What a server that only revokes refresh tokens answers for an access
/// token (RFC 7009 §2.2.1).
const UNSUPPORTED_TOKEN_TYPE: &str = "unsupported_token_type";

/// Asks the declared revocation endpoint to revoke `tokens`, in order; the
/// first that is not revoked stops and says why. An access token the server
/// does not revoke, once a refresh token was, is not one: the grant went
/// with it, the access token lasts until it expires (§2.1).
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
    let mut grant_revoked = false;
    for held in tokens {
        let hint = if held.refresh {
            "refresh_token"
        } else {
            "access_token"
        };
        let params = [("token", held.token), ("token_type_hint", hint)];
        match post_form(declaration, &endpoint, &params, client_secret, policy).await {
            TokenAnswer::Granted(_) => grant_revoked |= held.refresh,
            TokenAnswer::Refused { code, .. }
                if grant_revoked
                    && !held.refresh
                    && code.as_deref() == Some(UNSUPPORTED_TOKEN_TYPE) => {}
            TokenAnswer::Refused { said, .. } | TokenAnswer::Failed(said) => return Err(said),
        }
    }
    Ok(())
}
