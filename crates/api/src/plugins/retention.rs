//! Retention applied (DATA_MODEL §5, ADR-185): once an hour, in one process
//! at a time, the sessions ended thirty days ago are deleted, the episodes
//! recorded a year ago and the memories past their own term, the personal
//! memories of the accounts disabled thirty days ago. Each company that lost
//! memories finds it in its audit log, written by the platform in the
//! deletion's own statement: how many, and why, never what they said.

use std::time::Duration;

use mimorya_db::Database;
use mimorya_db::repositories::retention::{
    PurgedMemories, purge_ended_sessions, purge_retained_memories,
};
use tokio::time::Instant;

/// The name of the pass's lock: one process at a time (ADR-086).
pub const RETENTION_SWEEP: &str = "retention";
/// How often a process runs the pass.
const EVERY: Duration = Duration::from_secs(60 * 60);

/// What a pass deleted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Retained {
    pub sessions: i32,
    /// The companies that lost memories, and how many.
    pub memories: Vec<PurgedMemories>,
}

/// When this process last ran the pass.
#[derive(Debug, Default)]
pub struct Retention {
    last: Option<Instant>,
}

impl Retention {
    /// The pass, at the first call and then once an hour.
    pub async fn pass(&mut self, db: &Database) {
        let now = Instant::now();
        if !due(self.last, now) {
            return;
        }
        self.last = Some(now);
        match apply_retention(db).await {
            Ok(Some(retained)) if retained != Retained::default() => {
                let memories: i32 = retained
                    .memories
                    .iter()
                    .map(|lost| lost.aged + lost.departed)
                    .sum();
                tracing::info!(sessions = retained.sessions, memories, "Retention applied");
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(error = %error, "Retention pass failed"),
        }
    }
}

/// Whether a process that last ran the pass at `last` runs it at `now`:
/// never yet, or an hour ago.
pub fn due(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= EVERY)
}

/// One pass, unless another process runs one: nothing then. Each purge is
/// tried whatever the other did, its failure logged.
pub async fn apply_retention(db: &Database) -> anyhow::Result<Option<Retained>> {
    let Some(sweep) = db.try_sweep_lock(RETENTION_SWEEP).await? else {
        return Ok(None);
    };
    let sessions = purge_ended_sessions(db.pool())
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Ended session purge failed");
            0
        });
    let memories = purge_retained_memories(db.pool())
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(error = %error, "Retained memory purge failed");
            Vec::new()
        });
    sweep.release().await;
    Ok(Some(Retained { sessions, memories }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_at_once_then_once_an_hour() {
        let start = Instant::now();
        assert!(due(None, start));
        assert!(!due(Some(start), start));
        assert!(!due(Some(start), start + EVERY - Duration::from_secs(1)));
        assert!(due(Some(start), start + EVERY));
        // A clock that went back runs nothing.
        assert!(!due(Some(start + EVERY), start));
    }
}
