//! Rate limits shared by every process (ADR-088): the authentication limits,
//! counted in PostgreSQL, so that several `api` processes count a client's
//! sign-ins, an account's failures or an address's reset links together.
//! Only a digest of each key (client address, e-mail address) and value
//! leaves the process. A count that cannot be read is an error: these limits
//! guard the sign-in, which reads the same database.

use std::time::Duration;

use mimorya_db::Database;
use mimorya_db::repositories::rate_limits::{
    hit_distinct_rate_limit, hit_rate_limit, release_rate_limit,
};
use sha2::{Digest, Sha256};

fn digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

fn as_count(limit: u32) -> i32 {
    i32::try_from(limit).unwrap_or(i32::MAX)
}

/// So many hits per key and fixed window, shared by every process.
pub struct SharedLimiter {
    db: Database,
    /// Which limit: its windows are its own.
    name: &'static str,
    limit: i32,
    window: Duration,
}

impl SharedLimiter {
    pub fn new(db: Database, name: &'static str, limit: u32, window: Duration) -> Self {
        Self {
            db,
            name,
            limit: as_count(limit),
            window,
        }
    }

    pub fn per_minute(db: Database, name: &'static str, limit: u32) -> Self {
        Self::new(db, name, limit, Duration::from_secs(60))
    }

    /// Counts one hit for `key`; `false` when the limit is reached.
    pub async fn allow(&self, key: &str) -> anyhow::Result<bool> {
        let seconds = self.window.as_secs_f64();
        let key = digest(key);
        Ok(hit_rate_limit(self.db.pool(), self.name, &key, self.limit, seconds).await?)
    }

    /// Gives back a hit `allow` counted, e.g. once a login succeeded. One
    /// that cannot be given back stays counted: it is logged, not raised.
    pub async fn release(&self, key: &str) {
        let seconds = self.window.as_secs_f64();
        let key = digest(key);
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
    name: &'static str,
    limit: i32,
    window: Duration,
}

impl SharedDistinctLimiter {
    pub fn per_minute(db: Database, name: &'static str, limit: u32) -> Self {
        Self {
            db,
            name,
            limit: as_count(limit),
            window: Duration::from_secs(60),
        }
    }

    /// Counts `value` for `client`, once per window; `false` when it is a
    /// new one past the limit.
    pub async fn allow(&self, client: &str, value: &str) -> anyhow::Result<bool> {
        let (key, value) = (digest(client), digest(value));
        let seconds = self.window.as_secs_f64();
        let pool = self.db.pool();
        Ok(hit_distinct_rate_limit(pool, self.name, &key, &value, self.limit, seconds).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_digest_of_a_key_leaves_the_process() {
        let key = digest("nadia.benali@neo-maintenance.example");
        assert_eq!(key, digest("nadia.benali@neo-maintenance.example"));
        assert_ne!(key, digest("paul.renaud@neo-maintenance.example"));
        assert!(!key.windows(5).any(|w| w == b"nadia"));
    }

    #[test]
    fn a_limit_too_large_for_the_database_saturates() {
        assert_eq!(as_count(10), 10);
        assert_eq!(as_count(u32::MAX), i32::MAX);
    }
}
