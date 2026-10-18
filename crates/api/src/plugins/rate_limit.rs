//! Fixed-window rate limiting, in memory (single instance, like the event
//! bus). A global limit per client address protects every route; logins are
//! also limited per address and, for failures, per account.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::request_context::RequestContext;
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

    /// Whether `key` already reached the limit in the current window (counts nothing).
    pub fn exhausted(&self, key: &str) -> bool {
        self.with_window(key, |count, limit| *count >= limit)
    }

    /// Counts one event for `key`, e.g. a failed login.
    pub fn record(&self, key: &str) {
        self.with_window(key, |count, _| *count += 1);
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
    /// Every request, per client address.
    pub global: RateLimiter,
    /// Login attempts, per client address.
    pub login: RateLimiter,
    /// Failed logins, per account (email): addresses can be shared or forged,
    /// accounts cannot.
    pub failed_logins: RateLimiter,
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
    fn counts_events_without_consuming_checks() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        assert!(!limiter.exhausted("a"));
        limiter.record("a");
        limiter.record("a");
        assert!(limiter.exhausted("a"));
        assert!(!limiter.exhausted("b"));
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
