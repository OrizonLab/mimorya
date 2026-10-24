//! Fixed-window rate limiting. The global limit per client address, which
//! protects every route, is counted in memory, per process: several `api`
//! processes each count their own (ADR-016, ADR-088), sparing every request
//! a write to the database. The authentication limits are shared by every
//! process (`shared_limit`, ADR-088): logins per client address and, for
//! failures, per account; reset links per e-mail address; the addresses
//! asked about and the sign-ins through an identity provider, per client
//! address.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::request_context::RequestContext;
use super::shared_limit::{SharedDistinctLimiter, SharedLimiter};
use crate::app::AppState;
use crate::http::ApiError;

/// Above this many tracked clients, expired windows are purged.
const PURGE_THRESHOLD: usize = 10_000;

pub struct RateLimiter {
    limit: u32,
    window: Duration,
    windows: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            windows: Mutex::new(HashMap::new()),
        }
    }

    pub fn per_minute(limit: u32) -> Self {
        Self::new(limit, Duration::from_secs(60))
    }

    /// Counts one request for `key`; `false` when the limit is reached.
    pub fn allow(&self, key: &str) -> bool {
        self.with_window(key, |count, limit| {
            if *count >= limit {
                return false;
            }
            *count += 1;
            true
        })
    }

    /// Gives back a slot taken by `allow`, e.g. once a login succeeded.
    pub fn release(&self, key: &str) {
        self.with_window(key, |count, _| *count = count.saturating_sub(1));
    }

    fn with_window<T>(&self, key: &str, apply: impl FnOnce(&mut u32, u32) -> T) -> T {
        let now = Instant::now();
        let mut windows = self.windows.lock().expect("rate limiter");
        if windows.len() > PURGE_THRESHOLD {
            windows.retain(|_, (start, _)| now.duration_since(*start) < self.window);
        }
        let entry = windows.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= self.window {
            *entry = (now, 0);
        }
        apply(&mut entry.1, self.limit)
    }
}

pub struct RateLimits {
    /// Every request, per client address, in this process.
    pub global: RateLimiter,
    /// Login attempts, per client address.
    pub login: SharedLimiter,
    /// The addresses asked about to sign in through an identity provider,
    /// per client address, by its discovery or its start: each tells whether
    /// an address has an account tied to a provider. As many as logins,
    /// spending nothing of their limit; asking again about an address
    /// already counted costs nothing (ADR-063).
    pub sso_addresses: SharedDistinctLimiter,
    /// The start and the return of a sign-in through an identity provider,
    /// per client address: twice the login limit, a sign-in taking both
    /// (ADR-063).
    pub sso_sign_in: SharedLimiter,
    /// Login attempts per account (email), given back when the login succeeds:
    /// only failures count. Addresses can be shared or multiplied, accounts cannot.
    pub failed_logins: SharedLimiter,
    /// Password reset links asked for, per e-mail address, known or not.
    pub reset_requests: SharedLimiter,
}

pub async fn global_rate_limit(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let key = request
        .extensions()
        .get::<RequestContext>()
        .map_or_else(|| "unknown".to_string(), RequestContext::client_key);
    if !state.limits.global.allow(&key) {
        return ApiError::RateLimited.into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_each_client_separately_within_a_window() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.allow("a"));
        assert!(limiter.allow("a"));
        assert!(!limiter.allow("a"));
        assert!(limiter.allow("b"));
    }

    #[test]
    fn gives_back_released_slots() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert!(limiter.allow("a"));
        limiter.release("a");
        assert!(limiter.allow("a"));
        assert!(!limiter.allow("a"));
    }

    #[test]
    fn opens_a_new_window_once_the_previous_one_ended() {
        let limiter = RateLimiter::new(1, Duration::from_millis(5));
        assert!(limiter.allow("a"));
        assert!(!limiter.allow("a"));
        std::thread::sleep(Duration::from_millis(10));
        assert!(limiter.allow("a"));
    }
}
