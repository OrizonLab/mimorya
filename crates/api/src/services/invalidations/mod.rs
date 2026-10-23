//! What a process changes in the caches every process keeps (ADR-085): a
//! company's access, model or embedding settings to load again, its
//! memory's indexing due, a connector instance to release or to try again
//! at once. The process that made the change applies it to its own caches
//! at once and announces it to the others on a PostgreSQL channel, once its
//! transaction committed (`announce`); each of them applies the others'
//! (`listener.rs`). Best effort: a notice lost, the caches' short lifetime
//! (a minute at most) bounds how long a process serves the old value; one
//! whose listener lost its connection forgets its caches once connected
//! again.

mod listener;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

pub use listener::share_invalidations;

/// Announcements waiting to be sent, beyond which new ones are dropped.
const CAPACITY: usize = 256;

/// A change to apply to the caches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Invalidation {
    /// The company's organization, roles or policies (`AccessService`).
    Access(Uuid),
    /// The company's model (`LlmService`).
    Llm(Uuid),
    /// The company's embedding model (`EmbeddingService`).
    Embeddings(Uuid),
    /// A memory was written or approved: a pass of the indexer is due.
    Indexing,
    /// The connector instance changed, was disabled or deleted.
    Connector(Uuid),
    /// The connector instance answered a test: tried again at once.
    ConnectorRetry(Uuid),
}

/// An invalidation as announced: by which process.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Announced {
    origin: Uuid,
    #[serde(flatten)]
    invalidation: Invalidation,
}

/// Announces this process's invalidations to the others.
#[derive(Clone)]
pub struct Invalidations {
    sender: mpsc::Sender<Invalidation>,
    /// Whether this process hears the others' announcements.
    listening: Arc<AtomicBool>,
}

/// The invalidations announced, on their way to the other processes.
pub struct Outgoing {
    origin: Uuid,
    receiver: mpsc::Receiver<Invalidation>,
}

impl Invalidations {
    /// The process's announcer, and what it hands over to be sent.
    pub fn new() -> (Self, Outgoing) {
        let origin = Uuid::new_v4();
        let (sender, receiver) = mpsc::channel(CAPACITY);
        let invalidations = Self {
            sender,
            listening: Arc::new(AtomicBool::new(false)),
        };
        (invalidations, Outgoing { origin, receiver })
    }

    /// Whether this process hears the others' announcements: connected to
    /// the channel, its caches emptied once.
    pub fn listening(&self) -> bool {
        self.listening.load(Ordering::Relaxed)
    }

    fn set_listening(&self, listening: bool) {
        self.listening.store(listening, Ordering::Relaxed);
    }

    /// Announces `invalidation`, already applied here, to the other
    /// processes, after those announced before it.
    pub fn announce(&self, invalidation: Invalidation) {
        if self.sender.try_send(invalidation).is_err() {
            tracing::debug!(?invalidation, "Invalidation not announced");
        }
    }
}

/// `invalidation` as `origin` announces it.
fn payload(origin: Uuid, invalidation: Invalidation) -> String {
    let announced = Announced {
        origin,
        invalidation,
    };
    serde_json::to_string(&announced).unwrap_or_default()
}

/// What another process announced, if it is one: `None` for this one's
/// own announcements, and anything unreadable.
fn announced_by_another(origin: Uuid, payload: &str) -> Option<Invalidation> {
    let announced: Announced = serde_json::from_str(payload).ok()?;
    (announced.origin != origin).then_some(announced.invalidation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_process_s_announcements_alone_are_applied() {
        let (here, there) = (Uuid::new_v4(), Uuid::new_v4());
        let tenant = Uuid::new_v4();
        for invalidation in [
            Invalidation::Access(tenant),
            Invalidation::Indexing,
            Invalidation::ConnectorRetry(tenant),
        ] {
            let payload = payload(there, invalidation);
            assert_eq!(announced_by_another(here, &payload), Some(invalidation));
            assert_eq!(announced_by_another(there, &payload), None);
        }
        assert_eq!(
            payload(here, Invalidation::Llm(tenant)),
            format!(r#"{{"origin":"{here}","kind":"llm","id":"{tenant}"}}"#)
        );
        assert_eq!(announced_by_another(here, "{}"), None);
        assert_eq!(announced_by_another(here, "pas du json"), None);
    }
}
