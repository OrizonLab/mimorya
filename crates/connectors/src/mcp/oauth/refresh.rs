//! An authorization's access token renewed with its refresh token (ADR-171,
//! RFC 6749 §6): at the declared token endpoint, for the declared resource,
//! the client proving itself as declared (`token_request`). The grant is
//! lost only when the server says so (`invalid_grant`, or the client no
//! longer accepted, `invalid_client`), or when it grants tokens the platform
//! could not keep; any other refusal, an answer that does not read as
//! tokens, or a server that does not answer, is tried again later (as
//! ADR-117 does for linked accounts). A refresh token not given again is
//! kept.
use serde_json::Value;

use super::declaration::OAuthDeclaration;
use super::exchange::{Granted, granted};
use super::token_request::{TokenAnswer, request_tokens};
use crate::outbound::NetworkPolicy;

/// Why a renewal gave no tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenewalFailure {
    /// The grant is no longer good: the authorization is lost.
    Refused(String),
    /// The server could not answer: to be tried again.
    Unavailable(String),
}

/// New tokens for the refresh token kept, if the server still honours it.
pub async fn refresh_tokens(
    declaration: &OAuthDeclaration,
    refresh_token: &str,
    client_secret: Option<&str>,
    policy: NetworkPolicy,
) -> Result<Granted, RenewalFailure> {
    let grant = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ];
    match request_tokens(declaration, &grant, client_secret, policy).await {
        TokenAnswer::Granted(body) if reads_as_tokens(&body) => {
            granted(&body, true).map_err(RenewalFailure::Refused)
        }
        TokenAnswer::Granted(_) => Err(RenewalFailure::Unavailable(
            "Réponse du point de jeton illisible".into(),
        )),
        TokenAnswer::Refused { code, said } if lost(code.as_deref()) => {
            Err(RenewalFailure::Refused(said))
        }
        TokenAnswer::Refused { said, .. } | TokenAnswer::Failed(said) => {
            Err(RenewalFailure::Unavailable(said))
        }
    }
}

/// Whether the server says the grant, or the client, is no longer good.
fn lost(code: Option<&str>) -> bool {
    matches!(code, Some("invalid_grant" | "invalid_client"))
}

/// Whether the answer is a JSON object naming an access token: one that
/// does not may be a proxy's page, tried again later.
fn reads_as_tokens(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .is_some_and(|answer| answer.get("access_token").is_some_and(Value::is_string))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::oauth::ClientAuth;

    #[test]
    fn a_renewal_can_be_awaited_by_a_request_handler() {
        fn sendable<T: Send>(_: T) {}
        let declaration = OAuthDeclaration {
            revocation_endpoint: None,
            registered: None,
            holder: Default::default(),
            issuer: "https://auth.example.com".into(),
            resource: "https://mcp.example.com/mcp".into(),
            authorization_endpoint: "https://auth.example.com/authorize".into(),
            token_endpoint: "https://auth.example.com/token".into(),
            names_issuer_on_return: false,
            client_id: "mimorya".into(),
            client_auth: ClientAuth::ClientSecretBasic,
            scopes: Vec::new(),
        };
        sendable(refresh_tokens(
            &declaration,
            "renouvellement",
            Some("secret"),
            NetworkPolicy::default(),
        ));
    }

    #[test]
    fn only_the_server_saying_the_grant_is_gone_loses_it() {
        assert!(lost(Some("invalid_grant")) && lost(Some("invalid_client")));
        for other in [
            None,
            Some("invalid_request"),
            Some("temporarily_unavailable"),
        ] {
            assert!(!lost(other), "{other:?}");
        }
        assert!(reads_as_tokens(br#"{"access_token":"jeton-abc-123"}"#));
        for page in [&b"<html>"[..], b"{}", br#"{"access_token":12}"#] {
            assert!(!reads_as_tokens(page));
        }
    }

    #[test]
    fn a_renewed_token_keeps_the_refresh_token_it_was_not_given_again() {
        let body = br#"{"access_token":"jeton-abc-123","token_type":"Bearer","expires_in":3600}"#;
        let kept = granted(body, true).expect("renewed");
        assert_eq!(kept.refresh_token, None);
        assert!(granted(body, false).is_err());
    }
}
