//! Cross-cutting concerns applied around the routes: request context and
//! logging, rate limiting (per process, and shared for the sign-in),
//! CSRF protection, session resolution, security
//! headers, the audit of refused administration changes, the periodic
//! maintenance (stale runs, expired reset links), the indexing of memories
//! by meaning, scheduled tasks, execution leases, run events relayed
//! across processes and the run queue.

pub mod csrf;
pub mod maintenance;
pub mod memory_indexing;
pub mod rate_limit;
pub mod refusal_audit;
pub mod request_context;
pub mod run_event_relay;
pub mod run_leases;
pub mod run_queue;
pub mod scheduled_tasks;
pub mod security_headers;
pub mod session;
pub mod shared_limit;

pub use rate_limit::{RateLimiter, RateLimits};
pub use request_context::RequestContext;
pub use session::{Auth, AuthPendingPasswordChange, MaybeAuth};
pub use shared_limit::{SharedDistinctLimiter, SharedLimiter};
