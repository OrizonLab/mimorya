//! A connector instance's start, run in the background (ADR-177): it goes
//! on whether the runs that wait for it still do or not, and leaves its
//! outcome in the cache for them and for the runs that follow. A live
//! instance it started is kept unless the instance was invalidated while
//! it ran, or a newer one went live meanwhile; a failure is remembered,
//! told by the first run that reads it.

use std::future::Future;
use std::sync::Arc;

use mimorya_connectors::ConnectorRuntime;
use tokio::sync::{Mutex, watch};
use tokio::time::Instant;

use super::{RETRY_AFTER, SessionKey, Slot, State, Version, close_in_background, people, release};

/// Why a run goes without an instance still starting when its patience ran
/// out: its tools serve the runs that follow.
pub const STILL_STARTING: &str = "Le connecteur démarre encore : ses outils serviront aux \
     demandes suivantes";

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
        Fut: Future<Output = Result<ConnectorRuntime, String>>,
    {
        let started = started.await;
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
        let stale = state.stale(id, ticket);
        let newer =
            matches!(state.slots.get(&id), Some(Slot::Live { version: v, .. }) if *v >= version);
        if stale || newer {
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
                    let failed = Slot::Failed {
                        version,
                        until: Instant::now() + RETRY_AFTER,
                        message,
                        fresh: true,
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
