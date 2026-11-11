//! Retention applied (DATA_MODEL §5, ADR-185, ADR-186): once an hour, in one
//! process at a time, the sessions ended thirty days ago are deleted, and,
//! past each company's own durations (ADR-195; the platform's without its
//! choice: a year, thirty days, twenty-four months), the episodes and the
//! memories past their own term, the personal memories of the accounts
//! disabled, and the conversations idle, whole; and, past the platform's
//! five years, the consumption (ADR-197). Each company that lost memories,
//! conversations or consumption finds it in its audit log, written by the
//! platform in the deletion's own statement: how many, and why, never what
//! they said.

use std::future::Future;
use std::time::Duration;

use mimorya_db::Database;
use mimorya_db::repositories::retention::{
    PurgedConversations, PurgedMemories, PurgedUsage, purge_ended_sessions,
    purge_idle_conversations, purge_old_usage, purge_retained_memories,
};
use tokio::time::Instant;
use uuid::Uuid;

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
        let outcome = apply_retention(db).await;
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
                tracing::info!(
                    sessions = retained.sessions,
                    memories,
                    conversations,
                    usage,
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
/// tried whatever the others did, its failure logged.
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
    sweep.release().await;
    Ok(Some(Retained {
        sessions,
        memories,
        conversations,
        usage,
    }))
}

/// What a company lost to a purge that goes a batch at a time.
trait Lost {
    fn tenant_id(&self) -> Uuid;
    /// What the batch size counts.
    fn count(&self) -> i32;
    /// Adds what the same company lost to another statement.
    fn add(&mut self, other: &Self);
}

impl Lost for PurgedConversations {
    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    fn count(&self) -> i32 {
        self.conversations
    }

    fn add(&mut self, other: &Self) {
        self.conversations += other.conversations;
        self.messages += other.messages;
    }
}

impl Lost for PurgedUsage {
    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    fn count(&self) -> i32 {
        self.records
    }

    fn add(&mut self, other: &Self) {
        self.records += other.records;
    }
}

/// What `purge` deleted a batch of `size` at a time, while a batch comes
/// back full and the pass has `batches` statements left: each company
/// once, what it lost in all. A failed statement ends it, logged as `what`.
async fn in_batches<T, F, Fut>(what: &str, batches: usize, size: i32, mut purge: F) -> Vec<T>
where
    T: Lost,
    F: FnMut() -> Fut,
    Fut: Future<Output = sqlx::Result<Vec<T>>>,
{
    let mut lost: Vec<T> = Vec::new();
    for _ in 0..batches {
        let batch = match purge().await {
            Ok(batch) => batch,
            Err(error) => {
                tracing::warn!(error = %error, "{what} purge failed");
                break;
            }
        };
        let deleted: i32 = batch.iter().map(Lost::count).sum();
        for company in batch {
            match lost
                .iter_mut()
                .find(|known| known.tenant_id() == company.tenant_id())
            {
                Some(known) => known.add(&company),
                None => lost.push(company),
            }
        }
        if deleted < size {
            break;
        }
    }
    lost
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
