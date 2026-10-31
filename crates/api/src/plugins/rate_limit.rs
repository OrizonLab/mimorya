//! Fixed-window rate limiting. The global limit per client address, which
//! protects every route, is counted in memory, per process: several `api`
//! processes each count their own (ADR-016, ADR-088), sparing every request
//! a write to the database. The authentication limits are shared by every
//! process (`shared_limit`, ADR-088): logins per client address and, for
//! failures, per account; reset links per e-mail address; the addresses
//! asked about and the sign-ins through an identity provider, per client
//! address. Their keys leave the process as their HMAC under the server's
//! key (ADR-152).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::request_context::RequestContext;
use super::shared_limit::{KeyDigest, SharedDistinctLimiter, SharedLimiter};
use crate::app::AppState;
use crate::config::{AppConfig, Environment};
use crate::http::ApiError;
use mimorya_db::Database;

/// Above this many tracked clients, expired windows are purged.
const PURGE_THRESHOLD: usize = 10_000;
/// Requests per client address and minute, in each process.
pub(crate) const GLOBAL_RATE_LIMIT_PER_MINUTE: u32 = 600;
/// Failed logins allowed per account before it is throttled for the window.
const FAILED_LOGINS_PER_ACCOUNT: u32 = 10;
pub(crate) const FAILED_LOGIN_WINDOW: Duration = Duration::from_secs(15 * 60);
/// Password reset links an address may ask for in the window.
const RESET_REQUESTS_PER_ADDRESS: u32 = 3;
pub(crate) const RESET_REQUEST_WINDOW: Duration = Duration::from_secs(60 * 60);

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

impl RateLimits {
    /// A process's limits: the global one its own, `global` requests a
    /// minute; the authentication ones shared, their keys digested under
    /// the key derived from the master key, or unkeyed without one, which
    /// production is warned of.
    pub fn new(db: &Database, config: &AppConfig, global: u32) -> anyhow::Result<Self> {
        let digest = Arc::new(KeyDigest::of(config.secrets.as_deref())?);
        if !digest.is_keyed() && config.env == Environment::Production {
            tracing::warn!(
                "No MIMORYA_MASTER_KEY: the rate limit keys are kept as unkeyed digests, \
                 which an address can be guessed against by whoever reads the database"
            );
        }
        let login = config.login_rate_limit_per_minute;
        let shared = |name, limit, window| {
            SharedLimiter::new(db.clone(), digest.clone(), name, limit, window)
        };
        Ok(Self {
            global: RateLimiter::per_minute(global),
            login: shared("login", login, Duration::from_secs(60)),
            sso_addresses: SharedDistinctLimiter::per_minute(
                db.clone(),
                digest.clone(),
                "sso_addresses",
                login,
            ),
            sso_sign_in: shared(
                "sso_sign_in",
                login.saturating_mul(2),
                Duration::from_secs(60),
            ),
            failed_logins: shared(
                "failed_logins",
                FAILED_LOGINS_PER_ACCOUNT,
                FAILED_LOGIN_WINDOW,
            ),
            reset_requests: shared(
                "reset_requests",
                RESET_REQUESTS_PER_ADDRESS,
                RESET_REQUEST_WINDOW,
            ),
        })
    }
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
