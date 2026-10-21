//! Distinct values per client within a fixed window, in memory: the e-mail
//! addresses a client asks about to sign in through an identity provider
//! (ADR-063). Asking again about one already counted costs nothing, so the
//! sign-in page may ask twice for the same address, and its start follow.
//! Only a digest of each value is kept, for the window's length.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// Above this many tracked clients, expired windows are purged.
const PURGE_THRESHOLD: usize = 10_000;

type Seen = HashSet<[u8; 32]>;

pub struct DistinctLimiter {
    limit: usize,
    window: Duration,
    windows: Mutex<HashMap<String, (Instant, Seen)>>,
}

impl DistinctLimiter {
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit: usize::try_from(limit).unwrap_or(usize::MAX),
            window,
            windows: Mutex::new(HashMap::new()),
        }
    }

    pub fn per_minute(limit: u32) -> Self {
        Self::new(limit, Duration::from_secs(60))
    }

    /// Counts `value` for `client`, once per window; `false` when it is a
    /// new one past the limit.
    pub fn allow(&self, client: &str, value: &str) -> bool {
        let digest: [u8; 32] = Sha256::digest(value.as_bytes()).into();
        let now = Instant::now();
        let mut windows = self.windows.lock().expect("distinct limiter");
        if windows.len() > PURGE_THRESHOLD {
            windows.retain(|_, (start, _)| now.duration_since(*start) < self.window);
        }
        let (start, seen) = windows
            .entry(client.to_string())
            .or_insert_with(|| (now, Seen::new()));
        if now.duration_since(*start) >= self.window {
            *start = now;
            seen.clear();
        }
        if seen.contains(&digest) {
            return true;
        }
        if seen.len() >= self.limit {
            return false;
        }
        seen.insert(digest);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_each_value_once_per_client() {
        let limiter = DistinctLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.allow("a", "nadia@neo.example"));
        assert!(limiter.allow("a", "nadia@neo.example"));
        assert!(limiter.allow("a", "paul@neo.example"));
        assert!(!limiter.allow("a", "personne@ailleurs.example"));
        // Those already counted still answer.
        assert!(limiter.allow("a", "nadia@neo.example"));
        assert!(limiter.allow("b", "personne@ailleurs.example"));
    }

    #[test]
    fn opens_a_new_window_once_the_previous_one_ended() {
        let limiter = DistinctLimiter::new(1, Duration::from_millis(5));
        assert!(limiter.allow("a", "un"));
        assert!(!limiter.allow("a", "deux"));
        std::thread::sleep(Duration::from_millis(10));
        assert!(limiter.allow("a", "deux"));
        assert!(!limiter.allow("a", "un"));
    }
}
