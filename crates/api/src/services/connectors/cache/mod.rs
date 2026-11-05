//! The live connector instances of every tenant, by instance id and version
//! (`updated_at`), the starts in progress, and the failed starts remembered
//! for a while. A run that needs an instance while it starts waits for that
//! start rather than make its own (ADR-049): a server that does not answer
//! delays the runs of one start, then one run per retry delay, not every
//! run. A start invalidated while it ran (instance changed, disabled or
//! deleted) is released, never kept. A live instance whose session closed
//! for good (a local server that exited, a server that no longer answers a
//! ping, ADR-164) is released and started again by the next run that needs
//! it, or remembered as a failed start (`closed`, ADR-163). On a server
//! each person authorizes, each person has their own instance, started
//! with their own authorization and versioned by it too; those are bounded
//! in number, the least recently used released first, and released after a
//! while without a run (`people`, ADR-174).

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use mimorya_connectors::ConnectorRuntime;
use tokio::sync::{Mutex, watch};
use tokio::time::Instant;
use uuid::Uuid;

mod closed;
mod keys;
mod people;

use closed::closed;
pub use keys::{SessionKey, Version};
pub use people::{MAX_PERSON_SESSIONS, PERSON_IDLE};

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
        version: Version,
        runtime: Arc<ConnectorRuntime>,
        /// When it started: a session closed soon after is a failed start.
        since: Instant,
        /// When a run was last given it.
        used: Instant,
    },
    Failed {
        version: Version,
        until: Instant,
        message: String,
    },
    /// A start in progress: `done` changes or closes when it ends, or when
    /// the run that makes it gives it up.
    Starting {
        version: Version,
        done: watch::Receiver<()>,
    },
}

#[derive(Default)]
struct State {
    slots: HashMap<SessionKey, Slot>,
    /// Bumped by each invalidation.
    generation: u64,
    /// The generation at which each instance was last invalidated.
    invalidated: HashMap<Uuid, u64>,
    /// The generation at which each person's instance was last forgotten,
    /// and when: forgotten in turn once no start could still read it.
    forgotten: HashMap<SessionKey, (u64, Instant)>,
}

impl State {
    /// Whether the instance, or the person's, was invalidated after the
    /// caller read it.
    fn stale(&self, key: SessionKey, ticket: u64) -> bool {
        let instance = self.invalidated.get(&key.instance);
        let person = self.forgotten.get(&key).map(|(at, _)| at);
        instance.into_iter().chain(person).any(|at| *at > ticket)
    }

    /// Releases every slot of the instance, the people's included.
    fn remove_instance(&mut self, id: Uuid, which: impl Fn(&Slot) -> bool) {
        let keys: Vec<SessionKey> = self
            .slots
            .iter()
            .filter(|(key, slot)| key.instance == id && which(slot))
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            release(self.slots.remove(&key));
        }
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
        key: impl Into<SessionKey>,
        version: impl Into<Version>,
        ticket: u64,
        start: F,
    ) -> Result<Arc<ConnectorRuntime>, Unavailable>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<ConnectorRuntime, String>>,
    {
        let (id, version) = (key.into(), version.into());
        // Dropped when this start ends, or is given up: it wakes the others.
        let (_finished, done) = loop {
            let mut waiting = {
                let mut state = self.state.lock().await;
                people::release_idle(&mut state, Instant::now());
                if let Some(failed) = closed(&mut state, id, version) {
                    return Err(failed);
                }
                let stale = state.stale(id, ticket);
                match state.slots.get_mut(&id) {
                    Some(Slot::Live {
                        version: v,
                        runtime,
                        used,
                        ..
                    }) if *v == version => {
                        *used = Instant::now();
                        return Ok(runtime.clone());
                    }
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
                ..
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
                if id.person.is_some() {
                    people::make_room(&mut state, id);
                }
                let now = Instant::now();
                let live = Slot::Live {
                    version,
                    runtime: runtime.clone(),
                    since: now,
                    used: now,
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
    /// deletion, the people's too, and forgets a remembered failure.
    pub async fn invalidate(&self, id: Uuid) {
        let mut state = self.state.lock().await;
        state.generation += 1;
        let generation = state.generation;
        state.invalidated.insert(id, generation);
        state.remove_instance(id, |_| true);
    }

    /// Forgets a remembered failure (the instance answered a test): the next
    /// run tries again at once.
    pub async fn retry_now(&self, id: Uuid) {
        let mut state = self.state.lock().await;
        state.remove_instance(id, |slot| matches!(slot, Slot::Failed { .. }));
    }

    /// Releases a person's instance of a server, and forgets their failed
    /// start: their authorization went (ADR-174). A start of it in
    /// progress is not kept either.
    pub async fn forget_person(&self, id: Uuid, person: Uuid) {
        let mut state = self.state.lock().await;
        state.generation += 1;
        let key = SessionKey::person(id, person);
        let forgotten = (state.generation, Instant::now());
        state.forgotten.insert(key, forgotten);
        release(state.slots.remove(&key));
    }

    /// How many people's instances this process keeps live.
    pub async fn person_sessions(&self) -> usize {
        let state = self.state.lock().await;
        let live = |(key, slot): &(&SessionKey, &Slot)| {
            key.person.is_some() && matches!(slot, Slot::Live { .. })
        };
        state.slots.iter().filter(live).count()
    }

    /// Releases the people's instances no run was given for a while, and
    /// forgets their failed starts once over.
    pub async fn release_idle(&self) {
        people::release_idle(&mut *self.state.lock().await, Instant::now());
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
