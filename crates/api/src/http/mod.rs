//! HTTP plumbing shared by every route: errors, extractors and wire formats.

pub mod error;
pub mod extract;
pub mod format;

pub use error::{ApiError, ApiResult};
pub use extract::{Body, PathId, Query};
pub use format::{iso, iso_opt, nullable, trimmed, trimmed_nullable, trimmed_opt};
