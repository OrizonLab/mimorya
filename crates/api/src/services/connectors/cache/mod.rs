//! The live connector instances of every tenant, by instance id and version
//! (`updated_at`), the starts in progress, and the failed starts remembered
//! for a while. A run that needs an instance while it starts waits for that
//! start rather than make its own (ADR-049): a server that does not answer
//! delays the runs of one start, then one run per retry delay, not every
//! run. A start invalidated while it ran (instance changed, disabled or
//! deleted) is released, never kept. A live instance whose session closed
//! for good (a local server that exited) is released and started again by
//! the next run that needs it.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use mimorya_connectors::ConnectorRuntime;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

/// How long a failed start is remembered before the next run tries again.
pub const RETRY_AFTER: Duration = Duration::from_secs(60);

/// Where an unavailability comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Found now: logged and recorded on the instance.
    Fresh,
    /// Remembered from an earlier start: already logged and recorded.
    Remembered,
    /// The instance changed during its start: not a failure of any
    /// configuration, neither logged nor recorded.
    Changed,
}

/// Why an instance is not available to a run.
#[derive(Debug, Clone)]
pub struct Unavailable {
    pub message: String,
    pub origin: Origin,
}

impl Unavailable {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            origin: Origin::Fresh,
        }
    }

    fn changed() -> Self {
        Self {
            message: "Le connecteur a changé pendant son démarrage".into(),
            origin: Origin::Changed,
        }
    }
}

enum Slot {
    Live {
        version: DateTime<Utc>,
        runtime: Arc<ConnectorRuntime>,
    },
    Failed {
        version: DateTime<Utc>,
        until: Instant,
        message: String,
    },
    /// A start in progress: `done` changes or closes when it ends, or when
    /// the run that makes it gives it up.
    Starting {
        version: DateTime<Utc>,
        done: watch::Receiver<()>,
    },
}

#[derive(Default)]
struct State {
    slots: HashMap<Uuid, Slot>,
    /// Bumped by each invalidation.
    generation: u64,
    /// The generation at which each instance was last invalidated.
    invalidated: HashMap<Uuid, u64>,
}

impl State {
    /// Whether the instance was invalidated after the caller read it.
    fn stale(&self, id: Uuid, ticket: u64) -> bool {
        self.invalidated.get(&id).is_some_and(|at| *at > ticket)
    }
}

#[derive(Default)]
pub struct RuntimeCache {
    state: Mutex<State>,
}

impl RuntimeCache {
    /// Taken before reading the instances: a start of what was read is
    /// stale if the instance is invalidated after this ticket.
    pub async fn ticket(&self) -> u64 {
        self.state.lock().await.generation
    }

    /// The live instance of this version, a remembered failure, the outcome
    /// of the start in progress, or a new start. Starts run without the
    /// lock: a slow server delays its own instance only.
    pub async fn get_or_start<F, Fut>(
        &self,
        id: Uuid,
        version: DateTime<Utc>,
        ticket: u64,
        start: F,
    ) -> Result<Arc<ConnectorRuntime>, Unavailable>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<ConnectorRuntime, String>>,
    {
        // Dropped when this start ends, or is given up: it wakes the others.
        let (_finished, done) = loop {
            let mut waiting = {
                let mut state = self.state.lock().await;
                if matches!(state.slots.get(&id), Some(Slot::Live { version: v, runtime }) if *v == version && runtime.is_closed())
                {
                    tracing::warn!(instance_id = %id, "A kept connector's session closed: starting it again");
                    release(state.slots.remove(&id));
                }
                let stale = state.stale(id, ticket);
                match state.slots.get(&id) {
                    Some(Slot::Live {
                        version: v,
                        runtime,
                    }) if *v == version => return Ok(runtime.clone()),
                    Some(Slot::Failed {
                        version: v,
                        until,
                        message,
                    }) if *v == version && Instant::now() < *until => {
                        return Err(Unavailable {
                            message: message.clone(),
                            origin: Origin::Remembered,
                        });
                    }
                    // The instance of this version (same configuration, same
                    // token) serves even a caller whose reading is stale; any
                    // other start would be discarded.
                    _ if stale => return Err(Unavailable::changed()),
                    // Unless the run making it gave it up (its sender is gone).
                    Some(Slot::Starting { version: v, done })
                        if *v == version && done.has_changed().is_ok() =>
                    {
                        done.clone()
                    }
                    _ => {
                        let (finished, done) = watch::channel(());
                        let starting = Slot::Starting {
                            version,
                            done: done.clone(),
                        };
                        release(state.slots.insert(id, starting));
                        break (finished, done);
                    }
                }
            };
            // Ended or given up: what it left is read again.
            let _ = waiting.changed().await;
        };
        let started = start().await;
        let mut state = self.state.lock().await;
        // Defensive: the outcome below replaces this start's slot anyway
        // (an invalidation already removed it).
        if matches!(state.slots.get(&id), Some(Slot::Starting { done: d, .. }) if d.same_channel(&done))
        {
            state.slots.remove(&id);
        }
        let stale = state.stale(id, ticket);
        let newer = match state.slots.get(&id) {
            Some(Slot::Live {
                version: v,
                runtime,
            }) if *v >= version => Some(runtime.clone()),
            _ => None,
        };
        if stale || newer.is_some() {
            if let Ok(runtime) = started {
                close_in_background(Arc::new(runtime));
            }
            return match newer {
                Some(runtime) if !stale => Ok(runtime),
                _ => Err(Unavailable::changed()),
            };
        }
        match started {
            Ok(runtime) => {
                let runtime = Arc::new(runtime);
                let live = Slot::Live {
                    version,
                    runtime: runtime.clone(),
                };
                release(state.slots.insert(id, live));
                Ok(runtime)
            }
            Err(message) => {
                let failed = Slot::Failed {
                    version,
                    until: Instant::now() + RETRY_AFTER,
                    message: message.clone(),
                };
                release(state.slots.insert(id, failed));
                Err(Unavailable::new(message))
            }
        }
    }

    /// Releases the live instance after a change, a deactivation or a
    /// deletion, and forgets a remembered failure.
    pub async fn invalidate(&self, id: Uuid) {
        let mut state = self.state.lock().await;
        state.generation += 1;
        let generation = state.generation;
        state.invalidated.insert(id, generation);
        release(state.slots.remove(&id));
    }

    /// Forgets a remembered failure (the instance answered a test): the next
    /// run tries again at once.
    pub async fn retry_now(&self, id: Uuid) {
        let mut state = self.state.lock().await;
        if matches!(state.slots.get(&id), Some(Slot::Failed { .. })) {
            state.slots.remove(&id);
        }
    }

    /// Releases every live instance (shutdown).
    pub async fn close(&self) {
        let slots: Vec<Slot> = self
            .state
            .lock()
            .await
            .slots
            .drain()
            .map(|(_, s)| s)
            .collect();
        for slot in slots {
            if let Slot::Live { runtime, .. } = slot
                && let Some(closer) = &runtime.closer
            {
                closer.close().await;
            }
        }
    }
}

fn release(slot: Option<Slot>) {
    if let Some(Slot::Live { runtime, .. }) = slot {
        close_in_background(runtime);
    }
}

fn close_in_background(runtime: Arc<ConnectorRuntime>) {
    tokio::spawn(async move {
        if let Some(closer) = &runtime.closer {
            closer.close().await;
        }
    });
}

#[cfg(test)]
mod tests;
