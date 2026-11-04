//! Tokens the platform lets go of, revoked at the authorization server that
//! granted them (RFC 7009, ADR-172), within a bound: those of an
//! authorization the administration withdraws, and those a renewal brought
//! back once the authorization it was renewing was withdrawn, which no one
//! keeps. The refresh token first, under the declaration they were asked
//! under; why one was not revoked is said, never with a token.

use std::time::Duration;

use mimorya_connectors::mcp::oauth::{HeldToken, OAuthDeclaration, revoke_tokens};
use mimorya_connectors::outbound::NetworkPolicy;

/// How long the revocation endpoint is waited for, all tokens together.
pub const REVOCATION_BOUND: Duration = Duration::from_secs(10);

/// Said when the revocation endpoint did not answer within the bound.
const NOT_IN_TIME: &str = "Le point de révocation n’a pas répondu à temps";

/// The tokens of one grant, as they are let go of.
pub struct LetGo<'a> {
    pub access: &'a str,
    pub refresh: Option<&'a str>,
}

/// Asks the revocation endpoint `declared` names to revoke `tokens`, the
/// refresh token first, within `REVOCATION_BOUND`.
pub async fn revoke_within(
    declared: &OAuthDeclaration,
    tokens: LetGo<'_>,
    client_secret: Option<&str>,
    network: NetworkPolicy,
) -> Result<(), String> {
    let mut held = Vec::new();
    if let Some(refresh) = tokens.refresh {
        held.push(HeldToken {
            token: refresh,
            refresh: true,
        });
    }
    held.push(HeldToken {
        token: tokens.access,
        refresh: false,
    });
    let asked = revoke_tokens(declared, &held, client_secret, network);
    match tokio::time::timeout(REVOCATION_BOUND, asked).await {
        Ok(outcome) => outcome,
        Err(_) => Err(NOT_IN_TIME.to_string()),
    }
}
