//! Passwords (Argon2id) and session tokens. Hashing is CPU-bound: the async
//! helpers run it on the blocking thread pool.

use std::sync::LazyLock;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Duration;
use sha2::{Digest, Sha256};

pub const SESSION_COOKIE: &str = "mimorya_session";
pub const SESSION_IDLE: Duration = Duration::hours(12);
pub const SESSION_ABSOLUTE: Duration = Duration::days(7);
/// The sliding expiry is persisted at most this often.
pub const SESSION_TOUCH_INTERVAL: Duration = Duration::minutes(5);

/// OWASP-recommended Argon2id parameters (19 MiB, 2 iterations, 1 lane).
fn argon2() -> Argon2<'static> {
    let params = Params::new(19_456, 2, 1, None).expect("valid Argon2 parameters");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| anyhow::anyhow!("password hashing failed: {error}"))
}

/// Verified against when the account does not exist, so that response time
/// does not reveal whether an email is registered.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| hash_password("dummy-password-for-timing").expect("dummy hash"));

pub fn verify_password(password_hash: Option<&str>, password: &str) -> bool {
    let (stored, known) = match password_hash {
        Some(hash) => (hash, true),
        None => (DUMMY_HASH.as_str(), false),
    };
    let valid = PasswordHash::new(stored).is_ok_and(|parsed| {
        argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    });
    known && valid
}

pub async fn hash_password_async(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&password)).await?
}

pub async fn verify_password_async(password_hash: Option<String>, password: String) -> bool {
    tokio::task::spawn_blocking(move || verify_password(password_hash.as_deref(), &password))
        .await
        .unwrap_or(false)
}

/// Opaque 256-bit token; only its SHA-256 digest is stored.
pub fn new_session_token() -> (String, String) {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&token);
    (token, hash)
}

pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_passwords_and_never_accepts_a_missing_hash() {
        let hash = hash_password("secret-de-test").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify_password(Some(&hash), "secret-de-test"));
        assert!(!verify_password(Some(&hash), "autre"));
        assert!(!verify_password(None, "dummy-password-for-timing"));
        assert!(!verify_password(Some("not-a-hash"), "secret-de-test"));
    }

    #[test]
    fn issues_random_tokens_stored_as_digests() {
        let (token, hash) = new_session_token();
        let (other, _) = new_session_token();
        assert_eq!(token.len(), 43);
        assert_ne!(token, other);
        assert_eq!(hash, hash_token(&token));
        assert_eq!(hash.len(), 64);
    }
}
