//! The instances each person has of a server each person authorizes
//! (ADR-174): one per connector and person, so as many as the people who
//! run with it. Bounded in number per process, the one no run used for the
//! longest released to make room; released after `PERSON_IDLE` without a
//! run, so that a person who stopped using a server does not keep a
//! session, nor the token it holds, open here. An instance a run still
//! holds (its registry keeps the tools) is never released, neither for
//! being idle nor to make room: its time without a run starts when the run
//! lets it go, and the bound may be passed while every instance is held.
//! Their failed starts are forgotten once over, not to pile up one per
//! person, and so are the marks of their withdrawn authorizations. The
//! company's instances, one per connector, are neither counted nor
//! released here.

use std::sync::Arc;
use std::time::Duration;

use mimorya_connectors::ConnectorRuntime;
use tokio::time::Instant;

use super::{SessionKey, Slot, State, release};

/// How many people's instances a process keeps at most.
pub const MAX_PERSON_SESSIONS: usize = 200;

/// How long a person's instance is kept without a run.
pub const PERSON_IDLE: Duration = Duration::from_secs(15 * 60);

/// Whether a run holds the instance now: the instance itself, between its
/// start and its tools' registry, or its tools, in that registry; only the
/// cache holds either otherwise.
fn held(runtime: &Arc<ConnectorRuntime>) -> bool {
    Arc::strong_count(runtime) > 1 || runtime.tools.iter().any(|tool| Arc::strong_count(tool) > 1)
}

/// Releases the people's instances unused for `PERSON_IDLE` at `now`, a
/// held one counted as used now, and forgets their failed starts over at
/// `now`.
pub(super) fn release_idle(state: &mut State, now: Instant) {
    let mut gone = Vec::new();
    for (key, slot) in state.slots.iter_mut() {
        if key.person.is_none() {
            continue;
        }
        match slot {
            Slot::Live { runtime, used, .. } if held(runtime) => *used = now,
            Slot::Live { used, .. } if now.saturating_duration_since(*used) >= PERSON_IDLE => {
                gone.push(*key);
            }
            Slot::Failed { until, .. } if *until <= now => gone.push(*key),
            _ => {}
        }
    }
    for key in gone {
        release(state.slots.remove(&key));
    }
    // No start lasts that long: none can still have read what was forgotten.
    state
        .forgotten
        .retain(|_, (_, at)| now.saturating_duration_since(*at) < PERSON_IDLE);
}

/// Makes room for one more person's instance, `coming`: while the process
/// keeps `MAX_PERSON_SESSIONS` of them, the one no run used for the
/// longest, and none holds, is released.
pub(super) fn make_room(state: &mut State, coming: SessionKey) {
    loop {
        let live: Vec<(SessionKey, Instant, bool)> = state
            .slots
            .iter()
            .filter(|(key, _)| key.person.is_some() && **key != coming)
            .filter_map(|(key, slot)| match slot {
                Slot::Live { runtime, used, .. } => Some((*key, *used, held(runtime))),
                _ => None,
            })
            .collect();
        if live.len() < MAX_PERSON_SESSIONS {
            return;
        }
        let free = live.into_iter().filter(|(_, _, held)| !held);
        let Some((oldest, _, _)) = free.min_by_key(|(_, used, _)| *used) else {
            // Every one held by a run: the bound is passed until they end.
            return;
        };
        release(state.slots.remove(&oldest));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use chrono::Utc;
    use mimorya_connectors::ConnectorRuntime;
    use uuid::Uuid;

    use super::super::RuntimeCache;
    use super::*;

    /// Whether the instance it closes was released.
    struct Closer(Arc<AtomicBool>);

    #[async_trait::async_trait]
    impl mimorya_connectors::ConnectorCloser for Closer {
        async fn close(&self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// A start that counts itself and hands its release flag back.
    fn started(starts: &AtomicUsize) -> (ConnectorRuntime, Arc<AtomicBool>) {
        starts.fetch_add(1, Ordering::SeqCst);
        let released = Arc::new(AtomicBool::new(false));
        let runtime = ConnectorRuntime {
            tools: Vec::new(),
            closer: Some(Box::new(Closer(released.clone()))),
        };
        (runtime, released)
    }

    async fn released(flag: &AtomicBool) -> bool {
        for _ in 0..10_000 {
            if flag.load(Ordering::SeqCst) {
                return true;
            }
            tokio::task::yield_now().await;
        }
        false
    }

    /// Starts `key`'s instance if it is not live: its release flag.
    async fn start(
        cache: &RuntimeCache,
        key: SessionKey,
        starts: &Arc<AtomicUsize>,
    ) -> Option<Arc<AtomicBool>> {
        let flag = Arc::new(std::sync::Mutex::new(None));
        let version = chrono::DateTime::<Utc>::UNIX_EPOCH;
        let (starting, kept) = (starts.clone(), flag.clone());
        let got = cache
            .get_or_start(key, version, 0, None, move || async move {
                let (runtime, released) = started(&starting);
                *kept.lock().unwrap() = Some(released);
                Ok(runtime)
            })
            .await;
        assert!(got.is_ok());
        flag.lock().unwrap().take()
    }

    #[tokio::test]
    async fn each_person_has_an_instance_of_their_own() {
        let cache = RuntimeCache::default();
        let (server, marc, lea) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let starts = Arc::new(AtomicUsize::new(0));
        let company = start(&cache, server.into(), &starts).await.unwrap();
        let mine = start(&cache, SessionKey::person(server, marc), &starts).await;
        let hers = start(&cache, SessionKey::person(server, lea), &starts).await;
        assert!(mine.is_some() && hers.is_some());
        assert_eq!(starts.load(Ordering::SeqCst), 3);
        // Each kept: none started again.
        assert!(
            start(&cache, SessionKey::person(server, marc), &starts)
                .await
                .is_none()
        );
        assert!(start(&cache, server.into(), &starts).await.is_none());
        assert_eq!(starts.load(Ordering::SeqCst), 3);

        // Marc's authorization gone: his alone released.
        cache.forget_person(server, marc).await;
        assert!(released(mine.as_ref().unwrap()).await);
        assert!(!hers.as_ref().unwrap().load(Ordering::SeqCst));
        assert!(!company.load(Ordering::SeqCst));
        // The server changed: every instance of it released.
        cache.invalidate(server).await;
        assert!(released(hers.as_ref().unwrap()).await);
        assert!(released(&company).await);
    }

    #[tokio::test(start_paused = true)]
    async fn a_person_s_instance_goes_after_a_while_without_a_run() {
        let cache = RuntimeCache::default();
        let server = Uuid::new_v4();
        let (marc, lea) = (Uuid::new_v4(), Uuid::new_v4());
        let starts = Arc::new(AtomicUsize::new(0));
        let company = start(&cache, server.into(), &starts).await.unwrap();
        let mine = start(&cache, SessionKey::person(server, marc), &starts)
            .await
            .unwrap();
        let hers = start(&cache, SessionKey::person(server, lea), &starts)
            .await
            .unwrap();
        // Léa runs again before the end: hers kept, its time counted anew.
        tokio::time::advance(PERSON_IDLE - Duration::from_secs(60)).await;
        assert!(
            start(&cache, SessionKey::person(server, lea), &starts)
                .await
                .is_none()
        );
        tokio::time::advance(Duration::from_secs(60)).await;
        cache.release_idle().await;
        assert!(released(&mine).await);
        assert!(!hers.load(Ordering::SeqCst));
        // The company's is never released for being idle.
        assert!(!company.load(Ordering::SeqCst));
        // Marc's next run starts his again.
        assert!(
            start(&cache, SessionKey::person(server, marc), &starts)
                .await
                .is_some()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_person_no_run_used_for_the_longest_makes_room() {
        let cache = RuntimeCache::default();
        let server = Uuid::new_v4();
        let starts = Arc::new(AtomicUsize::new(0));
        let people: Vec<Uuid> = (0..MAX_PERSON_SESSIONS).map(|_| Uuid::new_v4()).collect();
        let mut flags = Vec::new();
        for person in &people {
            tokio::time::advance(Duration::from_millis(10)).await;
            let flag = start(&cache, SessionKey::person(server, *person), &starts).await;
            flags.push(flag.unwrap());
        }
        // The first person runs again: the second is now the oldest.
        assert!(
            start(&cache, SessionKey::person(server, people[0]), &starts)
                .await
                .is_none()
        );
        let company = start(&cache, server.into(), &starts).await.unwrap();
        let coming = start(&cache, SessionKey::person(server, Uuid::new_v4()), &starts).await;
        assert!(coming.is_some());
        assert!(released(&flags[1]).await);
        assert!(
            flags
                .iter()
                .enumerate()
                .all(|(at, flag)| at == 1 || !flag.load(Ordering::SeqCst))
        );
        // The company's instance does not count, nor is released.
        assert!(!company.load(Ordering::SeqCst));
    }

    /// A start whose instance has tools, the demo connector's: the tools,
    /// for a test to hold them as a run's registry would.
    async fn with_tools(
        cache: &RuntimeCache,
        key: SessionKey,
    ) -> (Arc<AtomicBool>, Vec<mimorya_core::tools::ToolRef>) {
        let demo = mimorya_connectors::connector_type("demo_interventions")
            .unwrap()
            .instantiate(&serde_json::json!({}), Default::default())
            .await
            .unwrap();
        let released = Arc::new(AtomicBool::new(false));
        let runtime = ConnectorRuntime {
            tools: demo.tools,
            closer: Some(Box::new(Closer(released.clone()))),
        };
        let version = chrono::DateTime::<Utc>::UNIX_EPOCH;
        let got = cache
            .get_or_start(key, version, 0, None, move || async move { Ok(runtime) })
            .await;
        let Ok(live) = got else {
            panic!("expected a live instance");
        };
        (released, live.tools.clone())
    }

    #[tokio::test(start_paused = true)]
    async fn an_instance_a_run_holds_is_never_released_while_it_does() {
        let cache = RuntimeCache::default();
        let server = Uuid::new_v4();
        let (marc, starts) = (Uuid::new_v4(), Arc::new(AtomicUsize::new(0)));
        let (flag, registry) = with_tools(&cache, SessionKey::person(server, marc)).await;
        // A run longer than the idle time: kept while it holds the tools.
        tokio::time::advance(PERSON_IDLE * 2).await;
        cache.release_idle().await;
        assert!(!released(&flag).await);
        // Nor released to make room: others not held go first.
        for _ in 0..MAX_PERSON_SESSIONS {
            let key = SessionKey::person(server, Uuid::new_v4());
            assert!(start(&cache, key, &starts).await.is_some());
        }
        assert!(!flag.load(Ordering::SeqCst));
        assert_eq!(cache.person_sessions().await, MAX_PERSON_SESSIONS);
        // The run over, its time without a run starts then.
        drop(registry);
        let given = cache
            .get_or_start(
                SessionKey::person(server, marc),
                chrono::DateTime::<Utc>::UNIX_EPOCH,
                0,
                None,
                || async { Err("jamais démarré".to_string()) },
            )
            .await;
        let Ok(given) = given else {
            panic!("expected the live instance");
        };
        // Given to a run, its tools not yet in a registry: held all the same.
        tokio::time::advance(PERSON_IDLE * 2).await;
        cache.release_idle().await;
        assert!(!flag.load(Ordering::SeqCst));
        drop(given);
        tokio::time::advance(PERSON_IDLE - Duration::from_secs(60)).await;
        cache.release_idle().await;
        assert!(!flag.load(Ordering::SeqCst));
        tokio::time::advance(Duration::from_secs(60)).await;
        cache.release_idle().await;
        assert!(released(&flag).await);
    }

    #[tokio::test]
    async fn the_bound_is_passed_only_while_runs_hold_every_instance() {
        let cache = RuntimeCache::default();
        let server = Uuid::new_v4();
        let mut held = Vec::new();
        for _ in 0..MAX_PERSON_SESSIONS {
            let key = SessionKey::person(server, Uuid::new_v4());
            held.push(with_tools(&cache, key).await);
        }
        let (flag, _registry) =
            with_tools(&cache, SessionKey::person(server, Uuid::new_v4())).await;
        assert_eq!(cache.person_sessions().await, MAX_PERSON_SESSIONS + 1);
        assert!(held.iter().all(|(flag, _)| !flag.load(Ordering::SeqCst)));
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn a_start_under_way_when_the_authorization_goes_is_not_kept() {
        let cache = RuntimeCache::default();
        let (server, marc) = (Uuid::new_v4(), Uuid::new_v4());
        let key = SessionKey::person(server, marc);
        let ticket = cache.ticket().await;
        let flag = Arc::new(std::sync::Mutex::new(None));
        let starts = Arc::new(AtomicUsize::new(0));
        let version = chrono::DateTime::<Utc>::UNIX_EPOCH;
        let (forgetting, kept) = (cache.clone(), flag.clone());
        let got = cache
            .get_or_start(key, version, ticket, None, move || async move {
                forgetting.forget_person(server, marc).await;
                let (runtime, released) = started(&starts);
                *kept.lock().unwrap() = Some(released);
                Ok(runtime)
            })
            .await;
        assert!(got.is_err());
        let started = flag.lock().unwrap().clone().unwrap();
        assert!(released(&started).await);
        assert_eq!(cache.person_sessions().await, 0);
    }
}
