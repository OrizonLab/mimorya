//! Cross-cutting concerns applied around the routes: request context and
//! logging, rate limiting, CSRF protection, session resolution, security
//! headers, the audit of refused administration changes, the periodic
//! maintenance (stale runs, expired reset links) and the indexing of
//! memories by meaning.

pub mod csrf;
pub mod distinct_limit;
pub mod maintenance;
pub mod memory_indexing;
pub mod rate_limit;
pub mod refusal_audit;
pub mod request_context;
pub mod security_headers;
pub mod session;

pub use distinct_limit::DistinctLimiter;
pub use rate_limit::{RateLimiter, RateLimits};
pub use request_context::RequestContext;
pub use session::{Auth, AuthPendingPasswordChange, MaybeAuth};
