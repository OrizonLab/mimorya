//! A link under way at the provider, kept by the browser until it comes
//! back (ADR-116): the connector, the person who started it, `state`,
//! `nonce` and PKCE verifier, sealed by the master key for the company,
//! which the value names in clear. Nothing is written to the database; the
//! code it awaits is redeemable once at the provider.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::secrets::SecretCipher;
use crate::services::sso::attempt::{open_value, seal_value};

pub const LINK_COOKIE: &str = "mimorya_link";
/// Sent back only to the links' addresses, the return among them.
const COOKIE_PATH: &str = "/api/account/links";
pub const LINK_LIFETIME: Duration = Duration::minutes(10);
/// The associated data's name: never a link's own.
const SEALED_AS: &str = "account_link.attempt";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkAttempt {
    pub connector_id: Uuid,
    /// Only the person who started it may finish it.
    pub user_id: Uuid,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
    pub expires_at: DateTime<Utc>,
}

pub fn seal(
    cipher: &SecretCipher,
    tenant_id: Uuid,
    attempt: &LinkAttempt,
) -> anyhow::Result<String> {
    seal_value(cipher, tenant_id, SEALED_AS, attempt)
}

/// The company and its attempt, if the value was sealed for it by this key,
/// is unaltered and not past its time.
pub fn open(cipher: &SecretCipher, value: &str, now: DateTime<Utc>) -> Option<(Uuid, LinkAttempt)> {
    let (tenant_id, attempt): (Uuid, LinkAttempt) = open_value(cipher, value, SEALED_AS)?;
    (attempt.expires_at > now).then_some((tenant_id, attempt))
}

pub fn link_cookie(value: &str, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{LINK_COOKIE}={value}; Max-Age={}; Path={COOKIE_PATH}; HttpOnly; SameSite=Lax{secure}",
        LINK_LIFETIME.num_seconds()
    )
}

pub fn cleared_link_cookie(secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{LINK_COOKIE}=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; \
         Path={COOKIE_PATH}; HttpOnly; SameSite=Lax{secure}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt(now: DateTime<Utc>) -> LinkAttempt {
        LinkAttempt {
            connector_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            state: "state".into(),
            nonce: "nonce".into(),
            pkce_verifier: "verifier".into(),
            expires_at: now + LINK_LIFETIME,
        }
    }

    #[test]
    fn opens_only_for_its_company_under_its_own_name_and_in_time() {
        let now = Utc::now();
        let cipher = SecretCipher::new(&[7; 32]).unwrap();
        let tenant = Uuid::new_v4();
        let attempt = attempt(now);
        let value = seal(&cipher, tenant, &attempt).unwrap();
        assert!(!value.contains("verifier") && !value.contains("nonce"));
        assert_eq!(open(&cipher, &value, now), Some((tenant, attempt.clone())));
        assert_eq!(open(&cipher, &value, attempt.expires_at), None);
        let other = value.replacen(&tenant.to_string(), &Uuid::new_v4().to_string(), 1);
        assert_eq!(open(&cipher, &other, now), None);
        // A sign-in's attempt is not a link's, though sealed by the same key.
        let sign_in = seal_value(&cipher, tenant, "sso.attempt", &attempt).unwrap();
        assert_eq!(open(&cipher, &sign_in, now), None);
    }

    #[test]
    fn the_cookie_goes_back_only_to_the_links_addresses() {
        let cookie = link_cookie("v", true);
        assert!(
            cookie.starts_with("mimorya_link=v; Max-Age=600; Path=/api/account/links; HttpOnly")
        );
        assert!(cookie.contains("SameSite=Lax") && cookie.ends_with("; Secure"));
        let cleared = cleared_link_cookie(false);
        assert!(cleared.contains("Max-Age=0") && cleared.contains("Path=/api/account/links"));
        assert!(!cleared.contains("Secure"));
    }
}
