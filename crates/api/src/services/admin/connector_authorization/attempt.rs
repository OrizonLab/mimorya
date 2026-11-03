//! An MCP server's authorization under way at its authorization server,
//! kept by the administrator's browser until it comes back (ADR-171): the
//! connector, the administrator who started it, `state`, the PKCE verifier
//! and a print of the declaration it was started under, sealed by the
//! master key for the company, which the value names in clear. Nothing is
//! written to the database; the code it awaits is redeemable once.

use chrono::{DateTime, Duration, Utc};
use mimorya_connectors::mcp::oauth::OAuthDeclaration;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::secrets::SecretCipher;
use crate::services::sso::{open_value, seal_value};

pub const AUTHORIZATION_COOKIE: &str = "mimorya_connector_oauth";
/// Sent back only to the return's address.
const COOKIE_PATH: &str = "/api/admin/connectors/oauth";
pub const AUTHORIZATION_LIFETIME: Duration = Duration::minutes(10);
/// The associated data's name: never a link's nor a sign-in's.
const SEALED_AS: &str = "connector_oauth.attempt";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationAttempt {
    pub connector_id: Uuid,
    /// Only the administrator who started it may finish it.
    pub user_id: Uuid,
    pub state: String,
    pub pkce_verifier: String,
    /// The print of the server's address and declaration (`declared_print`).
    pub declared: String,
    pub expires_at: DateTime<Utc>,
}

/// What the attempt keeps of the declaration it was started under: a
/// change of address or of declaration meanwhile ends it.
pub fn declared_print(url: &str, declaration: &OAuthDeclaration) -> String {
    let written = serde_json::to_string(declaration).unwrap_or_default();
    let digest = Sha256::new()
        .chain_update(url.as_bytes())
        .chain_update([0])
        .chain_update(written.as_bytes())
        .finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn seal(
    cipher: &SecretCipher,
    tenant_id: Uuid,
    attempt: &AuthorizationAttempt,
) -> anyhow::Result<String> {
    seal_value(cipher, tenant_id, SEALED_AS, attempt)
}

/// The company and its attempt, if the value was sealed for it by this key,
/// is unaltered and not past its time.
pub fn open(
    cipher: &SecretCipher,
    value: &str,
    now: DateTime<Utc>,
) -> Option<(Uuid, AuthorizationAttempt)> {
    let (tenant_id, attempt): (Uuid, AuthorizationAttempt) = open_value(cipher, value, SEALED_AS)?;
    (attempt.expires_at > now).then_some((tenant_id, attempt))
}

pub fn authorization_cookie(value: &str, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{AUTHORIZATION_COOKIE}={value}; Max-Age={}; Path={COOKIE_PATH}; HttpOnly; \
         SameSite=Lax{secure}",
        AUTHORIZATION_LIFETIME.num_seconds()
    )
}

pub fn cleared_authorization_cookie(secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{AUTHORIZATION_COOKIE}=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; \
         Path={COOKIE_PATH}; HttpOnly; SameSite=Lax{secure}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt(now: DateTime<Utc>) -> AuthorizationAttempt {
        AuthorizationAttempt {
            connector_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            state: "state".into(),
            pkce_verifier: "verifier".into(),
            declared: "print".into(),
            expires_at: now + AUTHORIZATION_LIFETIME,
        }
    }

    #[test]
    fn opens_only_for_its_company_under_its_own_name_and_in_time() {
        let now = Utc::now();
        let cipher = SecretCipher::new(&[7; 32]).unwrap();
        let tenant = Uuid::new_v4();
        let attempt = attempt(now);
        let value = seal(&cipher, tenant, &attempt).unwrap();
        assert!(!value.contains("verifier"));
        assert_eq!(open(&cipher, &value, now), Some((tenant, attempt.clone())));
        assert_eq!(open(&cipher, &value, attempt.expires_at), None);
        // A link's attempt is not an authorization's, though sealed alike.
        let link = seal_value(&cipher, tenant, "account_link.attempt", &attempt).unwrap();
        assert_eq!(open(&cipher, &link, now), None);
    }

    #[test]
    fn the_cookie_goes_back_only_to_the_return() {
        let cookie = authorization_cookie("v", true);
        assert!(cookie.starts_with(
            "mimorya_connector_oauth=v; Max-Age=600; Path=/api/admin/connectors/oauth; HttpOnly"
        ));
        assert!(cookie.ends_with("; Secure"));
        assert!(cleared_authorization_cookie(false).contains("Max-Age=0"));
    }
}
