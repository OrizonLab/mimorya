//! The identity of a person who authorized an MCP server for themself,
//! proven by its authorization server when the declaration asks for it
//! (ADR-175): the identity token granted with the tokens, verified against
//! the keys the server publishes as an OpenID provider (read at each
//! return: a person authorizes a server once in a while), for the declared
//! client and the nonce of the attempt; the account it proves must be the
//! person's own (`authorizing_identity`). Refused otherwise, the reason
//! said for the audit, never the token: the tokens granted are then kept
//! nowhere, and revoked (ADR-172).

use mimorya_connectors::mcp::oauth::Granted;
use mimorya_core::account_links::{LinkRefusal, authorizing_identity};

use super::attempt::AuthorizationAttempt;
use super::returned::{AttemptServer, AuthorizationFailure, Outcome, internal};
use crate::services::AccessContext;
use crate::services::admin::connectors::ConnectorAdmin;
use crate::services::sso::{IdToken, ProviderHttp, discover, token_claims, verify_for};

/// The server granted tokens without proving who authorized.
const UNPROVEN: &str = "Le serveur d’autorisation n’a pas donné de jeton d’identité";
/// The account proven is another address's than the person's.
pub const OTHER_ACCOUNT: &str = "Compte autorisé d’une autre adresse que celle de la personne";
/// The token names no address the server vouches for.
const UNIDENTIFIED: &str = "Le serveur d’autorisation ne garantit pas l’adresse du compte autorisé";

/// Whether the account the person authorized with is theirs, if the
/// declaration asks; `Ok` when it does not.
pub(super) async fn proven_identity(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    attempt: &AuthorizationAttempt,
    server: &AttemptServer,
    granted: &Granted,
) -> Outcome<()> {
    let declared = &server.declared;
    if !declared.verifies_identity {
        return Ok(());
    }
    let id = attempt.connector_id;
    let at = |reason: &str| AuthorizationFailure::new(Some(id), reason);
    // An attempt started before the declaration asked for it has no nonce:
    // its declaration changed, and the return is refused before.
    let nonce = attempt
        .nonce
        .as_deref()
        .ok_or_else(|| at("Autorisation commencée sans vérification de l’identité"))?;
    let token: IdToken = granted
        .id_token
        .as_deref()
        .ok_or_else(|| at(UNPROVEN))?
        .parse()
        .map_err(|_| at("Jeton d’identité illisible"))?;
    let http = ProviderHttp::new(admin.connectors.network()).map_err(|e| internal(id, e))?;
    let metadata = discover(&http, &declared.issuer).await.map_err(|failure| {
        at(&format!(
            "Serveur d’autorisation OpenID Connect : {failure}"
        ))
    })?;
    let secret = server.secret.as_deref();
    let claims = verify_for(metadata, &declared.client_id, secret, &token, nonce)
        .map_err(|unverified| at(&unverified.failure()))?;
    authorizing_identity(
        &declared.issuer,
        &token_claims(&claims),
        &access.profile.email,
    )
    .map(|_| ())
    .map_err(|refusal| match refusal {
        LinkRefusal::Unidentified => at(UNIDENTIFIED),
        LinkRefusal::NotTheirs => at(OTHER_ACCOUNT),
    })
}
