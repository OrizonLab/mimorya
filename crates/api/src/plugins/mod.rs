//! Cross-cutting concerns applied around the routes: request context and
//! logging, rate limiting, CSRF protection, session resolution, and the
//! periodic maintenance (stale runs, expired reset links).

pub mod csrf;
pub mod maintenance;
pub mod rate_limit;
pub mod request_context;
pub mod session;

pub use rate_limit::{RateLimiter, RateLimits};
pub use request_context::RequestContext;
pub use session::{Auth, AuthPendingPasswordChange, MaybeAuth};
