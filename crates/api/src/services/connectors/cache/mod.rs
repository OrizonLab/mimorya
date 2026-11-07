//! The live connector instances of every tenant, by instance id and version
//! (`updated_at`), the starts in progress, and the failed starts remembered
//! for a while. A run that needs an instance while it starts waits for that
//! start rather than make its own (ADR-049): a server that does not answer
//! delays the runs of one start, then one run per retry delay, not every
//! run. A start runs in the background, whoever asked for it (ADR-177): a
//! run waits for it at most its patience, if it has one, then goes without
//! the instance, which serves the runs that follow; one the run gave up
//! goes on. A failure is logged where it is found, by the start itself. A
//! newer instance, live, starting or failed, serves a run that read an
//! older one, and an older one's start never takes its place (`starting`).
//! A start invalidated while it ran (instance changed, disabled
//! or deleted) is released, never kept, and one that ends after the cache
//! closed too. A live instance whose session closed for good (a local
//! server that exited, a server that no longer answers a ping, ADR-164) is
//! released and started again by the next run that needs it, or
//! remembered as a failed start (`closed`, ADR-163). On a server
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
mod starting;

use closed::closed;
pub use keys::{SessionKey, Version};
pub use people::{MAX_PERSON_SESSIONS, PERSON_IDLE};
pub use starting::{INTERRUPTED, STILL_STARTING};

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
    /// Still starting when the run's patience ran out (ADR-177): no
    /// failure either; its tools serve the runs that follow.
    Starting,
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

    fn still_starting() -> Self {
        Self {
            message: STILL_STARTING.into(),
            origin: Origin::Starting,
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
    /// A start in progress, in the background whether the runs that wait
    /// for it still do or not: `done` closes once it left its outcome, or
    /// ended without one (ADR-177).
    Starting {
        version: Version,
        done: watch::Receiver<()>,
    },
}

impl Slot {
    /// Whether this entry, of a newer version than `version`, holds the
    /// place against it: live, still starting, or failed while its failure
    /// is remembered. It serves the runs that read `version`, and the
    /// outcome of `version`'s start never replaces it (ADR-177): one rule
    /// for both.
    fn newer_than(&self, version: Version, now: Instant) -> bool {
        match self {
            Slot::Live { version: v, .. } => *v > version,
            Slot::Starting { version: v, done } => *v > version && done.has_changed().is_ok(),
            Slot::Failed {
                version: v, until, ..
            } => *v > version && now < *until,
        }
    }
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
    /// Closed (shutdown): a start that ends now releases what it started.
    closed: bool,
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

/// Shared with the starts it runs in the background.
#[derive(Default, Clone)]
pub struct RuntimeCache {
    state: Arc<Mutex<State>>,
}

impl RuntimeCache {
    /// Taken before reading the instances: a start of what was read is
    /// stale if the instance is invalidated after this ticket.
    pub async fn ticket(&self) -> u64 {
        self.state.lock().await.generation
    }

    /// The live instance of this version, a remembered failure, the outcome
    /// of the start in progress, or a new start, run in the background:
    /// waited for at most `patience`, if any (ADR-177). Starts run without
    /// the lock: a slow server delays its own instance only.
    pub async fn get_or_start<F, Fut>(
        &self,
        key: impl Into<SessionKey>,
        version: impl Into<Version>,
        ticket: u64,
        patience: Option<Duration>,
        start: F,
    ) -> Result<Arc<ConnectorRuntime>, Unavailable>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<ConnectorRuntime, String>> + Send + 'static,
    {
        let (id, version) = (key.into(), version.into());
        let deadline = patience.map(|patience| Instant::now() + patience);
        let mut start = Some(start);
        loop {
            let mut waiting = {
                let mut state = self.state.lock().await;
                people::release_idle(&mut state, Instant::now());
                let stale = state.stale(id, ticket);
                if let Some(failed) = closed(&mut state, id, version, stale) {
                    return Err(failed);
                }
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
                    // Logged by its start, recorded by it if it is the
                    // company's (ADR-177).
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
                    // Unless its start ended without leaving an outcome.
                    Some(Slot::Starting { version: v, done })
                        if *v == version && done.has_changed().is_ok() =>
                    {
                        done.clone()
                    }
                    // A newer one serves the run as its own would, rather
                    // than it start an older one in its place: live, its
                    // start waited for, or its failure remembered.
                    Some(slot) if slot.newer_than(version, Instant::now()) => match slot {
                        Slot::Live { runtime, used, .. } => {
                            *used = Instant::now();
                            return Ok(runtime.clone());
                        }
                        Slot::Starting { done, .. } => done.clone(),
                        Slot::Failed { message, .. } => {
                            return Err(Unavailable {
                                message: message.clone(),
                                origin: Origin::Remembered,
                            });
                        }
                    },
                    _ => {
                        // Its own start ended, and another took its place.
                        let Some(start) = start.take() else {
                            return Err(Unavailable::changed());
                        };
                        let (finished, done) = watch::channel(());
                        let starting = Slot::Starting {
                            version,
                            done: done.clone(),
                        };
                        release(state.slots.insert(id, starting));
                        let started = starting::Start {
                            state: self.state.clone(),
                            id,
                            version,
                            ticket,
                            finished,
                            mine: done.clone(),
                        };
                        tokio::spawn(started.run(start()));
                        done
                    }
                }
            };
            // Ended, or ended without an outcome: what it left is read again.
            let ended = waiting.changed();
            match deadline {
                None => {
                    let _ = ended.await;
                }
                Some(deadline) => {
                    if tokio::time::timeout_at(deadline, ended).await.is_err() {
                        return Err(Unavailable::still_starting());
                    }
                }
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

    /// Releases every live instance (shutdown), and those of the starts
    /// still under way once they end.
    pub async fn close(&self) {
        let slots: Vec<Slot> = {
            let mut state = self.state.lock().await;
            state.closed = true;
            state.slots.drain().map(|(_, s)| s).collect()
        };
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
