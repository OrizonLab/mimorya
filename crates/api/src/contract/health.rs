//! Liveness of the API, for probes and the installation checks.

use serde::Serialize;
use ts_rs::TS;

/// `GET /health`: answered once the database answers.
#[derive(Debug, Serialize, TS)]
pub struct Health {
    /// Always `ok`.
    pub status: String,
    /// A platform model provider is configured (companies may have their own).
    pub llm: bool,
}
