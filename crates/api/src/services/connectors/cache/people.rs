//! The instances each person has of a server each person authorizes
//! (ADR-174): one per connector and person, so as many as the people who
//! run with it. Bounded in number per process, the one no run was given
//! for the longest released to make room; released after `PERSON_IDLE`
//! without a run, so that a person who stopped using a server does not
//! keep a session, nor the token it holds, open here. Their failed starts
//! are forgotten once over, not to pile up one per person. The company's
//! instances, one per connector, are neither counted nor released here.

use std::time::Duration;

use tokio::time::Instant;

use super::{SessionKey, Slot, State, release};

/// How many people's instances a process keeps at most.
pub const MAX_PERSON_SESSIONS: usize = 200;

/// How long a person's instance is kept without a run.
pub const PERSON_IDLE: Duration = Duration::from_secs(15 * 60);

/// Releases the people's instances unused for `PERSON_IDLE` at `now`, and
/// forgets their failed starts over at `now`.
pub(super) fn release_idle(state: &mut State, now: Instant) {
    let gone: Vec<SessionKey> = state
        .slots
        .iter()
        .filter(|(key, _)| key.person.is_some())
        .filter(|(_, slot)| match slot {
            Slot::Live { used, .. } => now.saturating_duration_since(*used) >= PERSON_IDLE,
            Slot::Failed { until, .. } => *until <= now,
            Slot::Starting { .. } => false,
        })
        .map(|(key, _)| *key)
        .collect();
    for key in gone {
        release(state.slots.remove(&key));
    }
}

/// Makes room for one more person's instance, `coming`: while the process
/// keeps `MAX_PERSON_SESSIONS` of them, the one no run was given for the
/// longest is released.
pub(super) fn make_room(state: &mut State, coming: SessionKey) {
    loop {
        let live: Vec<(SessionKey, Instant)> = state
            .slots
            .iter()
            .filter(|(key, _)| key.person.is_some() && **key != coming)
            .filter_map(|(key, slot)| match slot {
                Slot::Live { used, .. } => Some((*key, *used)),
                _ => None,
            })
            .collect();
        if live.len() < MAX_PERSON_SESSIONS {
            return;
        }
        let Some((oldest, _)) = live.into_iter().min_by_key(|(_, used)| *used) else {
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
        starts: &AtomicUsize,
    ) -> Option<Arc<AtomicBool>> {
        let flag = std::sync::Mutex::new(None);
        let version = chrono::DateTime::<Utc>::UNIX_EPOCH;
        let got = cache
            .get_or_start(key, version, 0, || async {
                let (runtime, released) = started(starts);
                *flag.lock().unwrap() = Some(released);
                Ok(runtime)
            })
            .await;
        assert!(got.is_ok());
        flag.into_inner().unwrap()
    }

    #[tokio::test]
    async fn each_person_has_an_instance_of_their_own() {
        let cache = RuntimeCache::default();
        let (server, marc, lea) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let starts = AtomicUsize::new(0);
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
        let starts = AtomicUsize::new(0);
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
        let starts = AtomicUsize::new(0);
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
}
