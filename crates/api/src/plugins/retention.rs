//! Retention applied (DATA_MODEL §5, ADR-185, ADR-186): once an hour, in one
//! process at a time, the sessions ended thirty days ago are deleted, and,
//! past each company's own durations (ADR-195; the platform's without its
//! choice: a year, thirty days, twenty-four months), the episodes and the
//! memories past their own term, the personal memories of the accounts
//! disabled, and the conversations idle, whole; and, past the platform's
//! five years, the consumption (ADR-197) and, with the key that seals the
//! audit log, the oldest places of each audit chain, their seals checked
//! and its verification kept (ADR-198; none without a master key, which
//! seals nothing either). Each company that lost
//! memories, conversations, consumption or audit entries finds it in its
//! audit log, written by the platform in the deletion's own statement: how
//! many, and why, never what they said.

use std::time::Duration;

use mimorya_db::Database;
use mimorya_db::repositories::retention::{
    PurgedAudit, PurgedConversations, PurgedMemories, PurgedUsage, purge_ended_sessions,
    purge_idle_conversations, purge_old_usage, purge_retained_memories,
};
use tokio::time::Instant;

use super::retention_batches::{Lost, in_batches};
use crate::services::audit_chain::{ChainKey, MAX_PURGE_BATCH, purge_old_places};

/// The name of the pass's lock: one process at a time (ADR-086).
pub const RETENTION_SWEEP: &str = "retention";
/// How often a process runs the pass.
const EVERY: Duration = Duration::from_secs(60 * 60);
/// The idle conversations one statement deletes, at most: a statement and
/// its transaction stay short.
pub const CONVERSATION_BATCH: i32 = 200;
/// The statements a pass gives them, at most: a backlog goes over several
/// passes, and the maintenance gets on with the rest meanwhile.
const CONVERSATION_BATCHES: usize = 25;
/// The consumption records one statement deletes, at most: small rows, a
/// short statement still.
pub const USAGE_BATCH: i32 = 5000;
/// The statements a pass gives them, at most.
const USAGE_BATCHES: usize = 20;
/// The audit entries one company's transactions delete in a statement, at
/// most, the database's bound.
pub const AUDIT_BATCH: i32 = MAX_PURGE_BATCH;
/// The statements a pass gives them, at most.
const AUDIT_BATCHES: usize = 20;

/// What a pass deleted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Retained {
    pub sessions: i32,
    /// The companies that lost memories, and how many.
    pub memories: Vec<PurgedMemories>,
    /// The companies that lost conversations, and how many.
    pub conversations: Vec<PurgedConversations>,
    /// The companies that lost consumption records, and how many.
    pub usage: Vec<PurgedUsage>,
    /// The companies whose audit chain lost its oldest places, and how many.
    pub audit: Vec<PurgedAudit>,
}

/// When this process last ran the pass, and the key it purges the audit
/// chains with.
#[derive(Default)]
pub struct Retention {
    last: Option<Instant>,
    key: Option<ChainKey>,
}

impl Retention {
    /// The pass of a process that seals the audit log with `key`, or none.
    pub fn new(key: Option<ChainKey>) -> Self {
        Self { last: None, key }
    }

    /// The pass, at the first call and then once an hour.
    pub async fn pass(&mut self, db: &Database) {
        let now = Instant::now();
        if !due(self.last, now) {
            return;
        }
        let outcome = apply_retention(db, self.key.as_ref()).await;
        self.last = settled(self.last, now, &outcome);
        match outcome {
            Ok(Some(retained)) if retained != Retained::default() => {
                let memories: i32 = retained
                    .memories
                    .iter()
                    .map(|lost| lost.aged + lost.departed)
                    .sum();
                let conversations: i32 = retained.conversations.iter().map(Lost::count).sum();
                let usage: i32 = retained.usage.iter().map(Lost::count).sum();
                let audit: i32 = retained.audit.iter().map(Lost::count).sum();
                tracing::info!(
                    sessions = retained.sessions,
                    memories,
                    conversations,
                    usage,
                    audit,
                    "Retention applied"
                );
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

/// When the pass counts as last run, after the one tried at `now` came to
/// `outcome`: done, here or by another process, it counts from `now`, an
/// hour until the next; failed before its lock was had (the database out of
/// reach), it counts as before, and is tried again at the next tick.
fn settled<T>(last: Option<Instant>, now: Instant, outcome: &anyhow::Result<T>) -> Option<Instant> {
    match outcome {
        Ok(_) => Some(now),
        Err(_) => last,
    }
}

/// One pass, unless another process runs one: nothing then. Each purge is
/// tried whatever the others did, its failure logged. The audit chains lose
/// their oldest places only with `key`, the one that seals them.
pub async fn apply_retention(
    db: &Database,
    key: Option<&ChainKey>,
) -> anyhow::Result<Option<Retained>> {
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
    let conversations = in_batches(
        "Idle conversation",
        CONVERSATION_BATCHES,
        CONVERSATION_BATCH,
        || purge_idle_conversations(db.pool(), CONVERSATION_BATCH),
    )
    .await;
    let usage = in_batches("Old usage", USAGE_BATCHES, USAGE_BATCH, || {
        purge_old_usage(db.pool(), USAGE_BATCH)
    })
    .await;
    let audit = match key {
        Some(key) => {
            in_batches("Old audit entries", AUDIT_BATCHES, AUDIT_BATCH, || {
                purge_old_places(db, key, AUDIT_BATCH, chrono::Utc::now())
            })
            .await
        }
        None => Vec::new(),
    };
    sweep.release().await;
    Ok(Some(Retained {
        sessions,
        memories,
        conversations,
        usage,
        audit,
    }))
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

    #[test]
    fn a_failed_pass_is_tried_again_at_the_next_tick() {
        let start = Instant::now();
        let later = start + EVERY;
        // Done here, or by another process: an hour from now.
        assert_eq!(settled(None, start, &Ok(Some(()))), Some(start));
        assert_eq!(settled(Some(start), later, &Ok(None::<()>)), Some(later));
        assert!(!due(
            settled(None, start, &Ok(())),
            start + Duration::from_secs(60)
        ));
        // Failed: as before, so due at once, the first time as later on.
        let failed: anyhow::Result<()> = Err(anyhow::anyhow!("database out of reach"));
        assert_eq!(settled(None, start, &failed), None);
        assert_eq!(settled(Some(start), later, &failed), Some(start));
        assert!(due(settled(Some(start), later, &failed), later));
    }
}
