//! A connector instance's start, run in the background (ADR-177): it goes
//! on whether the runs that wait for it still do or not, and leaves its
//! outcome in the cache for them and for the runs that follow. Its outcome
//! is kept unless the instance was invalidated while it ran, the cache
//! closed, or another took its place meanwhile: an instance of its version
//! live, or a newer entry as the runs read it (`Slot::newer_than`), a live
//! one judged first by its session. An older version never replaces it. A
//! failure is remembered, and logged here, where it is found, its own or a
//! live instance's that closed at once: no run may read it before it is
//! forgotten. A start that panics is a failure too.

use std::future::Future;
use std::sync::Arc;

use mimorya_connectors::ConnectorRuntime;
use tokio::sync::{Mutex, watch};
use tokio::time::Instant;

use super::{
    RETRY_AFTER, SessionKey, Slot, State, Version, close_in_background, closed, people, release,
};

/// Why a run goes without an instance still starting when its patience ran
/// out: its tools serve the runs that follow.
pub const STILL_STARTING: &str = "Le connecteur démarre encore : ses outils serviront aux \
     demandes suivantes";

/// A start that ended without an outcome (it panicked).
pub const INTERRUPTED: &str = "Le démarrage du connecteur s’est interrompu";

/// A start in progress, and where its outcome goes.
pub(super) struct Start {
    pub state: Arc<Mutex<State>>,
    pub id: SessionKey,
    pub version: Version,
    /// Taken before the instance was read: the start is stale if the
    /// instance was invalidated since.
    pub ticket: u64,
    /// Dropped once the outcome is left: it wakes the runs waiting for it.
    pub finished: watch::Sender<()>,
    /// Tells this start's slot from another's.
    pub mine: watch::Receiver<()>,
}

impl Start {
    pub async fn run<Fut>(self, started: Fut)
    where
        Fut: Future<Output = Result<ConnectorRuntime, String>> + Send + 'static,
    {
        // In a task of its own: a start that panics is a failed one.
        let started = match tokio::spawn(started).await {
            Ok(started) => started,
            Err(_) => Err(INTERRUPTED.to_string()),
        };
        let Self {
            state,
            id,
            version,
            ticket,
            finished,
            mine,
        } = self;
        let mut state = state.lock().await;
        // Its slot is replaced by the outcome below, unless an invalidation
        // already removed it, or another start took its place.
        if matches!(state.slots.get(&id), Some(Slot::Starting { done, .. }) if done.same_channel(&mine))
        {
            state.slots.remove(&id);
        }
        let stale = state.stale(id, ticket) || state.closed;
        // A live instance in place is judged first, as for a run: one whose
        // session closed for good is released, or replaced by its failure,
        // remembered, if it lived less than a minute (ADR-163); that failure,
        // found here, is logged here.
        if let Some(exited) = closed(&mut state, id, version) {
            let connector = id.instance;
            tracing::warn!(%connector, error = %exited.message, "Connector unavailable");
        }
        // Then the place is held as `get_or_start` reads it: by an instance
        // of its version live (another start's), or by a newer entry.
        let now = Instant::now();
        let held = state.slots.get(&id).is_some_and(|slot| {
            matches!(slot, Slot::Live { version: v, .. } if *v == version)
                || slot.newer_than(version, now)
        });
        if stale || held {
            if let Ok(runtime) = started {
                close_in_background(Arc::new(runtime));
            }
        } else {
            match started {
                Ok(runtime) => {
                    if id.person.is_some() {
                        people::make_room(&mut state, id);
                    }
                    let now = Instant::now();
                    let live = Slot::Live {
                        version,
                        runtime: Arc::new(runtime),
                        since: now,
                        used: now,
                    };
                    release(state.slots.insert(id, live));
                }
                Err(message) => {
                    let connector = id.instance;
                    tracing::warn!(%connector, error = %message, "Connector unavailable");
                    let failed = Slot::Failed {
                        version,
                        until: Instant::now() + RETRY_AFTER,
                        message,
                    };
                    release(state.slots.insert(id, failed));
                }
            }
        }
        // The runs waiting read what it left.
        drop(state);
        drop(finished);
    }
}
