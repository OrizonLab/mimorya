//! An authorization's access token renewed with its refresh token (ADR-171,
//! RFC 6749 §6): at the declared token endpoint, for the declared resource,
//! the client proving itself as declared (`token_request`). A refusal says
//! the grant is no longer good, and so does an answer the platform could
//! not keep (the server may have rotated the refresh token already): the
//! authorization is then lost. Told apart from a server that could not
//! answer, tried again later. A refresh token not given again is kept.

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
        TokenAnswer::Granted(body) => granted(&body, true).map_err(RenewalFailure::Refused),
        TokenAnswer::Refused(said) => Err(RenewalFailure::Refused(said)),
        TokenAnswer::Failed(said) => Err(RenewalFailure::Unavailable(said)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::oauth::ClientAuth;

    #[test]
    fn a_renewal_can_be_awaited_by_a_request_handler() {
        fn sendable<T: Send>(_: T) {}
        let declaration = OAuthDeclaration {
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
    fn a_renewed_token_keeps_the_refresh_token_it_was_not_given_again() {
        let body = br#"{"access_token":"jeton-abc-123","token_type":"Bearer","expires_in":3600}"#;
        let kept = granted(body, true).expect("renewed");
        assert_eq!(kept.refresh_token, None);
        assert!(granted(body, false).is_err());
    }
}
