//! Rate limits shared by every process (ADR-088): the authentication limits,
//! counted in PostgreSQL, so that several `api` processes count a client's
//! sign-ins, an account's failures or an address's reset links together.
//! Only a digest of each key (client address, e-mail address) and value
//! leaves the process: its HMAC under a key derived from the server's master
//! key (ADR-152), which whoever reads the database or a backup does not
//! hold, so that guessed addresses cannot be matched against it; taken with
//! the limit's name, so that one address's rows of two limits cannot be
//! linked either. A count that cannot be read is an error: these limits
//! guard the sign-in, which reads the same database.

use std::sync::Arc;
use std::time::Duration;

use mimorya_db::Database;
use mimorya_db::repositories::rate_limits::{
    hit_distinct_rate_limit, hit_rate_limit, release_rate_limit,
};
use ring::hmac;
use sha2::{Digest, Sha256};

use crate::secrets::SecretCipher;

/// The purpose of the key derived for the rate limits.
const RATE_LIMIT_PURPOSE: &[u8] = b"rate-limits";

/// How a key or value of a limit leaves the process: the HMAC of the
/// limit's name, a zero byte and the value, under the key derived from the
/// master key, the same for every process of a deployment; without a master
/// key, the unkeyed SHA-256 digest of the same, which an address can be
/// guessed against by whoever reads the database (said at startup).
pub enum KeyDigest {
    Keyed(hmac::Key),
    Unkeyed,
}

impl KeyDigest {
    pub fn of(secrets: Option<&SecretCipher>) -> anyhow::Result<Self> {
        Ok(match secrets {
            Some(cipher) => Self::Keyed(cipher.derived_key(RATE_LIMIT_PURPOSE)?),
            None => Self::Unkeyed,
        })
    }

    pub fn is_keyed(&self) -> bool {
        matches!(self, Self::Keyed(_))
    }

    /// The digest of `value` for the limit `limiter`, a name without a
    /// zero byte.
    pub fn digest(&self, limiter: &str, value: &str) -> [u8; 32] {
        let parts = [limiter.as_bytes(), &[0], value.as_bytes()];
        let mut digest = [0; 32];
        match self {
            Self::Keyed(key) => {
                let mut context = hmac::Context::with_key(key);
                parts.iter().for_each(|part| context.update(part));
                digest.copy_from_slice(context.sign().as_ref());
            }
            Self::Unkeyed => {
                let mut hasher = Sha256::new();
                parts.iter().for_each(|part| hasher.update(part));
                digest = hasher.finalize().into();
            }
        }
        digest
    }
}

/// A limit as the database counts it: one below its largest integer, kept
/// for the count past the limit.
fn as_count(limit: u32) -> i32 {
    i32::try_from(limit).map_or(i32::MAX - 1, |limit| limit.min(i32::MAX - 1))
}

/// So many hits per key and fixed window, shared by every process.
pub struct SharedLimiter {
    db: Database,
    digest: Arc<KeyDigest>,
    /// Which limit: its windows are its own.
    name: &'static str,
    limit: i32,
    window: Duration,
}

impl SharedLimiter {
    pub fn new(
        db: Database,
        digest: Arc<KeyDigest>,
        name: &'static str,
        limit: u32,
        window: Duration,
    ) -> Self {
        Self {
            db,
            digest,
            name,
            limit: as_count(limit),
            window,
        }
    }

    /// Counts one hit for `key`; `false` when the limit is reached.
    pub async fn allow(&self, key: &str) -> anyhow::Result<bool> {
        let seconds = self.window.as_secs_f64();
        let key = self.digest.digest(self.name, key);
        Ok(hit_rate_limit(self.db.pool(), self.name, &key, self.limit, seconds).await?)
    }

    /// Gives back a hit `allow` counted, e.g. once a login succeeded. One
    /// that cannot be given back stays counted: it is logged, not raised.
    pub async fn release(&self, key: &str) {
        let seconds = self.window.as_secs_f64();
        let key = self.digest.digest(self.name, key);
        let released =
            release_rate_limit(self.db.pool(), self.name, &key, self.limit, seconds).await;
        if let Err(error) = released {
            tracing::warn!(limiter = self.name, error = %error, "Rate limit hit not given back");
        }
    }
}

/// So many distinct values per key and fixed window, shared by every
/// process: a value already counted costs nothing more.
pub struct SharedDistinctLimiter {
    db: Database,
    digest: Arc<KeyDigest>,
    name: &'static str,
    limit: i32,
    window: Duration,
}

impl SharedDistinctLimiter {
    pub fn per_minute(
        db: Database,
        digest: Arc<KeyDigest>,
        name: &'static str,
        limit: u32,
    ) -> Self {
        Self {
            db,
            digest,
            name,
            limit: as_count(limit),
            window: Duration::from_secs(60),
        }
    }

    /// Counts `value` for `client`, once per window; `false` when it is a
    /// new one past the limit.
    pub async fn allow(&self, client: &str, value: &str) -> anyhow::Result<bool> {
        let digest = |part| self.digest.digest(self.name, part);
        let (key, value) = (digest(client), digest(value));
        let seconds = self.window.as_secs_f64();
        let pool = self.db.pool();
        Ok(hit_distinct_rate_limit(pool, self.name, &key, &value, self.limit, seconds).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NADIA: &str = "nadia.benali@neo-maintenance.example";

    fn keyed(byte: u8) -> KeyDigest {
        let cipher = SecretCipher::new(&[byte; 32]).unwrap();
        KeyDigest::of(Some(&cipher)).unwrap()
    }

    #[test]
    fn only_a_digest_of_a_key_leaves_the_process() {
        for digest in [keyed(7), KeyDigest::Unkeyed] {
            let key = digest.digest("login", NADIA);
            assert_eq!(key, digest.digest("login", NADIA));
            assert_ne!(
                key,
                digest.digest("login", "paul.renaud@neo-maintenance.example")
            );
            assert!(!key.windows(5).any(|w| w == b"nadia"));
            // One address's rows of two limits are not linked.
            assert_ne!(key, digest.digest("reset_requests", NADIA));
        }
    }

    #[test]
    fn a_key_s_digest_is_its_hmac_under_the_server_s_key() {
        let (one, same, other) = (keyed(7), keyed(7), keyed(8));
        assert!(one.is_keyed() && !KeyDigest::Unkeyed.is_keyed());
        // Every process of a deployment counts alike.
        assert_eq!(one.digest("login", NADIA), same.digest("login", NADIA));
        // Neither the unkeyed digest one could compute from a guess, nor
        // another deployment's.
        let guessed: [u8; 32] =
            Sha256::digest(b"login\0nadia.benali@neo-maintenance.example").into();
        assert_eq!(KeyDigest::Unkeyed.digest("login", NADIA), guessed);
        assert_ne!(one.digest("login", NADIA), guessed);
        assert_ne!(one.digest("login", NADIA), other.digest("login", NADIA));
    }

    #[test]
    fn a_limit_too_large_for_the_database_saturates() {
        assert_eq!(as_count(10), 10);
        assert_eq!(as_count(u32::MAX), i32::MAX - 1);
        let largest = u32::try_from(i32::MAX).unwrap();
        assert_eq!(as_count(largest), i32::MAX - 1);
        assert_eq!(as_count(largest - 1), i32::MAX - 1);
        assert_eq!(as_count(largest - 2), i32::MAX - 2);
    }
}
