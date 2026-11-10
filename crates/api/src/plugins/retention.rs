//! Retention applied (DATA_MODEL §5, ADR-185, ADR-186): once an hour, in one
//! process at a time, the sessions ended thirty days ago are deleted, and,
//! past each company's own durations (ADR-195; the platform's without its
//! choice: a year, thirty days, twenty-four months), the episodes and the
//! memories past their own term, the personal memories of the accounts
//! disabled, and the conversations idle, whole. Each company that lost
//! memories or conversations finds it in its audit log, written by the
//! platform in the deletion's own statement: how many, and why, never what
//! they said.

use std::time::Duration;

use mimorya_db::Database;
use mimorya_db::repositories::retention::{
    PurgedConversations, PurgedMemories, purge_ended_sessions, purge_idle_conversations,
    purge_retained_memories,
};
use tokio::time::Instant;

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

/// What a pass deleted.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Retained {
    pub sessions: i32,
    /// The companies that lost memories, and how many.
    pub memories: Vec<PurgedMemories>,
    /// The companies that lost conversations, and how many.
    pub conversations: Vec<PurgedConversations>,
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
                let conversations: i32 = retained
                    .conversations
                    .iter()
                    .map(|lost| lost.conversations)
                    .sum();
                tracing::info!(
                    sessions = retained.sessions,
                    memories,
                    conversations,
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
    let conversations = purge_conversations(db).await;
    sweep.release().await;
    Ok(Some(Retained {
        sessions,
        memories,
        conversations,
    }))
}

/// The idle conversations deleted a batch at a time, while a batch comes
/// back full and the pass has statements left; what each company lost.
async fn purge_conversations(db: &Database) -> Vec<PurgedConversations> {
    let mut lost: Vec<PurgedConversations> = Vec::new();
    for _ in 0..CONVERSATION_BATCHES {
        let batch = match purge_idle_conversations(db.pool(), CONVERSATION_BATCH).await {
            Ok(batch) => batch,
            Err(error) => {
                tracing::warn!(error = %error, "Idle conversation purge failed");
                break;
            }
        };
        let deleted: i32 = batch.iter().map(|company| company.conversations).sum();
        for company in batch {
            match lost
                .iter_mut()
                .find(|known| known.tenant_id == company.tenant_id)
            {
                Some(known) => {
                    known.conversations += company.conversations;
                    known.messages += company.messages;
                }
                None => lost.push(company),
            }
        }
        if deleted < CONVERSATION_BATCH {
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
