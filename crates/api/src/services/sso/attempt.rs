//! A sign-in under way at a provider, kept by the browser until it comes
//! back (ADR-063): the company in clear, then the provider, `state`,
//! `nonce`, PKCE verifier (OpenID Connect) or the request awaited (SAML,
//! ADR-068), and where the sign-in started, sealed by the master key with
//! the company as associated data, so that it opens only for it and cannot
//! be altered. A SAML provider posts its response from its own site: its
//! attempt travels in a cookie of its own, sent with such a request.
//! Nothing is written to the database; the code it awaits is redeemable
//! once at the provider.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use mimorya_db::repositories::secrets::StoredSecret;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::saml::SamlPending;
use crate::secrets::SecretCipher;

pub const ATTEMPT_COOKIE: &str = "mimorya_sso";
/// Sent back only to the return address.
const COOKIE_PATH: &str = "/api/auth/sso";
pub const SAML_ATTEMPT_COOKIE: &str = "mimorya_saml";
/// Sent back only to the SAML return address.
const SAML_COOKIE_PATH: &str = "/api/auth/saml";
pub const ATTEMPT_LIFETIME: Duration = Duration::minutes(10);
/// The associated data's name: never the name of a stored secret.
const SEALED_AS: &str = "sso.attempt";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub provider_id: Uuid,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
    pub expires_at: DateTime<Utc>,
    /// Started from the company's sign-in link, the only start that may
    /// create the account (ADR-067); not from an address.
    #[serde(default)]
    pub from_link: bool,
    /// SAML: the request the provider's response must answer; `state`,
    /// `nonce` and verifier are then empty.
    #[serde(default)]
    pub saml: Option<SamlPending>,
}

/// The cookie's value: `company.iv.tag.ciphertext`.
pub fn seal(cipher: &SecretCipher, tenant_id: Uuid, attempt: &Attempt) -> anyhow::Result<String> {
    let sealed = cipher.seal(tenant_id, SEALED_AS, &serde_json::to_string(attempt)?)?;
    Ok([
        tenant_id.to_string(),
        URL_SAFE_NO_PAD.encode(&sealed.iv),
        URL_SAFE_NO_PAD.encode(&sealed.auth_tag),
        URL_SAFE_NO_PAD.encode(&sealed.ciphertext),
    ]
    .join("."))
}

/// The company and its attempt, if the value was sealed for it by this key,
/// is unaltered and not past its time.
pub fn open(cipher: &SecretCipher, value: &str, now: DateTime<Utc>) -> Option<(Uuid, Attempt)> {
    let mut parts = value.split('.');
    let tenant_id = Uuid::parse_str(parts.next()?).ok()?;
    let mut decoded = || URL_SAFE_NO_PAD.decode(parts.next()?).ok();
    let (iv, auth_tag, ciphertext) = (decoded()?, decoded()?, decoded()?);
    let sealed = StoredSecret {
        ciphertext,
        iv,
        auth_tag,
        key_version: crate::secrets::KEY_VERSION,
    };
    let json = cipher.open(tenant_id, SEALED_AS, &sealed).ok()?;
    let attempt: Attempt = serde_json::from_str(&json).ok()?;
    (attempt.expires_at > now).then_some((tenant_id, attempt))
}

pub fn attempt_cookie(value: &str, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{ATTEMPT_COOKIE}={value}; Max-Age={}; Path={COOKIE_PATH}; HttpOnly; SameSite=Lax{secure}",
        ATTEMPT_LIFETIME.num_seconds()
    )
}

/// The provider posts its response from its own site: the cookie goes with
/// such a request (SameSite=None), which browsers allow only with Secure;
/// `localhost` counts as secure. Its path and lifetime are the attempt's.
pub fn saml_attempt_cookie(value: &str) -> String {
    format!(
        "{SAML_ATTEMPT_COOKIE}={value}; Max-Age={}; Path={SAML_COOKIE_PATH}; HttpOnly; \
         SameSite=None; Secure",
        ATTEMPT_LIFETIME.num_seconds()
    )
}

pub fn cleared_saml_attempt_cookie() -> String {
    format!(
        "{SAML_ATTEMPT_COOKIE}=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; \
         Path={SAML_COOKIE_PATH}; HttpOnly; SameSite=None; Secure"
    )
}

pub fn cleared_attempt_cookie(secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{ATTEMPT_COOKIE}=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; \
         Path={COOKIE_PATH}; HttpOnly; SameSite=Lax{secure}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher(byte: u8) -> SecretCipher {
        SecretCipher::new(&[byte; 32]).unwrap()
    }

    fn attempt(now: DateTime<Utc>) -> Attempt {
        Attempt {
            provider_id: Uuid::new_v4(),
            state: "state".into(),
            nonce: "nonce".into(),
            pkce_verifier: "verifier".into(),
            expires_at: now + ATTEMPT_LIFETIME,
            from_link: true,
            saml: None,
        }
    }

    #[test]
    fn opens_only_for_its_company_unaltered_and_in_time() {
        let now = Utc::now();
        let (cipher, tenant) = (cipher(7), Uuid::new_v4());
        let attempt = attempt(now);
        let value = seal(&cipher, tenant, &attempt).unwrap();
        assert!(!value.contains("verifier") && !value.contains("nonce"));
        assert_eq!(open(&cipher, &value, now), Some((tenant, attempt.clone())));
        // Past its time.
        assert_eq!(open(&cipher, &value, attempt.expires_at), None);
        // Another company named in clear, another key, an altered byte.
        let other = value.replacen(&tenant.to_string(), &Uuid::new_v4().to_string(), 1);
        assert_eq!(open(&cipher, &other, now), None);
        assert_eq!(open(&super::tests::cipher(8), &value, now), None);
        let mut altered = value.clone().into_bytes();
        let last = altered.len() - 1;
        altered[last] = if altered[last] == b'A' { b'B' } else { b'A' };
        assert_eq!(
            open(&cipher, &String::from_utf8(altered).unwrap(), now),
            None
        );
        for garbage in [
            "",
            "x",
            &format!("{tenant}.a.b"),
            &format!("{tenant}.!.!.!"),
        ] {
            assert_eq!(open(&cipher, garbage, now), None);
        }
    }

    #[test]
    fn the_cookie_goes_back_only_to_the_return_address() {
        let cookie = attempt_cookie("v", true);
        assert!(cookie.starts_with("mimorya_sso=v; Max-Age=600; Path=/api/auth/sso; HttpOnly"));
        assert!(cookie.contains("SameSite=Lax") && cookie.ends_with("; Secure"));
        let cleared = cleared_attempt_cookie(false);
        assert!(cleared.contains("Max-Age=0") && cleared.contains("Path=/api/auth/sso"));
        assert!(!cleared.contains("Secure"));
        // SAML: sent with the provider's cross-site post, only securely.
        let saml = saml_attempt_cookie("v");
        assert!(saml.starts_with("mimorya_saml=v; Max-Age=600; Path=/api/auth/saml; HttpOnly"));
        assert!(saml.ends_with("SameSite=None; Secure"));
        let cleared = cleared_saml_attempt_cookie();
        assert!(cleared.contains("Max-Age=0") && cleared.contains("Path=/api/auth/saml"));
    }
}
