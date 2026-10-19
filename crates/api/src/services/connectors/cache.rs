//! The live connector instances of every tenant, by instance id and version
//! (`updated_at`), and the failed starts remembered for a while: a server
//! that does not answer delays one run per retry delay, not every run. A
//! start invalidated while it ran (instance changed, disabled or deleted) is
//! released, never kept.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use mimorya_connectors::ConnectorRuntime;
use tokio::sync::Mutex;
use uuid::Uuid;

/// How long a failed start is remembered before the next run tries again.
pub const RETRY_AFTER: Duration = Duration::from_secs(60);

/// Why an instance is not available to a run.
#[derive(Debug, Clone)]
pub struct Unavailable {
    pub message: String,
    /// Remembered from an earlier start: already logged and recorded.
    pub remembered: bool,
}

impl Unavailable {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            remembered: false,
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
}

#[derive(Default)]
struct State {
    slots: HashMap<Uuid, Slot>,
    /// Bumped by each invalidation.
    generation: u64,
    /// The generation at which each instance was last invalidated.
    invalidated: HashMap<Uuid, u64>,
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

    /// The live instance of this version, a remembered failure, or a new
    /// start. Starts run without the lock: a slow server delays its own
    /// instance only.
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
        {
            let mut state = self.state.lock().await;
            match state.slots.get(&id) {
                Some(Slot::Live {
                    version: v,
                    runtime,
                }) if *v == version => {
                    return Ok(runtime.clone());
                }
                Some(Slot::Failed {
                    version: v,
                    until,
                    message,
                }) if *v == version && Instant::now() < *until => {
                    return Err(Unavailable {
                        message: message.clone(),
                        remembered: true,
                    });
                }
                _ => {}
            }
            release(state.slots.remove(&id));
        }
        let started = start().await;
        let mut state = self.state.lock().await;
        let stale = state.invalidated.get(&id).is_some_and(|at| *at > ticket);
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
                _ => Err(Unavailable::new(
                    "Le connecteur a changé pendant son démarrage",
                )),
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
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn unavailable(result: Result<Arc<ConnectorRuntime>, Unavailable>) -> Unavailable {
        match result {
            Err(unavailable) => unavailable,
            Ok(_) => panic!("expected the instance to be unavailable"),
        }
    }

    fn runtime() -> ConnectorRuntime {
        ConnectorRuntime {
            tools: Vec::new(),
            closer: None,
        }
    }

    #[tokio::test]
    async fn keeps_a_live_instance_and_remembers_a_failure_per_version() {
        let cache = RuntimeCache::default();
        let (id, v1, v2) = (
            Uuid::new_v4(),
            Utc::now(),
            Utc::now() + chrono::Duration::seconds(1),
        );
        let starts = AtomicUsize::new(0);
        let failing = || async {
            starts.fetch_add(1, Ordering::SeqCst);
            Err::<ConnectorRuntime, _>("injoignable".to_string())
        };
        let first = unavailable(cache.get_or_start(id, v1, 0, failing).await);
        assert!(!first.remembered);
        let again = unavailable(cache.get_or_start(id, v1, 0, failing).await);
        assert!(again.remembered);
        assert_eq!(again.message, "injoignable");
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        // A tested instance is tried again at once.
        cache.retry_now(id).await;
        assert!(!unavailable(cache.get_or_start(id, v1, 0, failing).await).remembered);
        assert_eq!(starts.load(Ordering::SeqCst), 2);

        // A new version starts again, then stays live.
        let working = || async {
            starts.fetch_add(1, Ordering::SeqCst);
            Ok(runtime())
        };
        let Ok(live) = cache.get_or_start(id, v2, 0, working).await else {
            panic!("expected a live instance");
        };
        let Ok(reused) = cache.get_or_start(id, v2, 0, working).await else {
            panic!("expected the live instance");
        };
        assert!(Arc::ptr_eq(&live, &reused));
        assert_eq!(starts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn never_keeps_a_start_invalidated_while_it_ran() {
        let cache = RuntimeCache::default();
        let (id, version) = (Uuid::new_v4(), Utc::now());
        let ticket = cache.ticket().await;
        let invalidated = unavailable(
            cache
                .get_or_start(id, version, ticket, || async {
                    cache.invalidate(id).await;
                    Ok(runtime())
                })
                .await,
        );
        assert!(invalidated.message.contains("a changé"));
        // Nothing was kept: the next run (a later ticket) starts again.
        let starts = AtomicUsize::new(0);
        let ticket = cache.ticket().await;
        let restarted = cache
            .get_or_start(id, version, ticket, || async {
                starts.fetch_add(1, Ordering::SeqCst);
                Ok(runtime())
            })
            .await;
        assert!(restarted.is_ok());
        assert_eq!(starts.load(Ordering::SeqCst), 1);
    }
}
