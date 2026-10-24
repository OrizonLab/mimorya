//! Whether this process executes runs (ADR-086): every `all` or `worker`
//! process does, once started, side by side with the others; an `api`
//! process never does (ADR-085). Until it has started, a process takes up,
//! claims, expires and reminds nothing.

use std::sync::atomic::Ordering;

use crate::services::runs::RunService;

/// Why an `api` process takes no run up (ADR-085).
pub const NOT_AN_EXECUTOR: &str =
    "An api process (MIMORYA_ROLE=api) executes no runs: a worker executes those it queues";

impl RunService {
    /// Whether this process executes runs: it has started to.
    pub fn executes(&self) -> bool {
        self.executing.load(Ordering::Relaxed)
    }

    /// Starts executing runs, for the life of this process, side by side
    /// with any other; refused in an `api` process.
    pub fn start_executing(&self) -> anyhow::Result<()> {
        if self.queues {
            anyhow::bail!(NOT_AN_EXECUTOR);
        }
        self.executing.store(true, Ordering::Relaxed);
        Ok(())
    }
}
