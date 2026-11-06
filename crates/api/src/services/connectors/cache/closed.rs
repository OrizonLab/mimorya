//! What a live instance whose session closed for good becomes (ADR-163):
//! a local server that exited, or a server that no longer answers a ping
//! (ADR-164). Released and started again by the run that finds it closed,
//! unless it lived less than `QUICK_EXIT`. It is then a failed start,
//! remembered as one: a server that exits, or stops answering, as soon as
//! started is not started again by every run. Its life lasts until its
//! session ended, when the session dates its end (ADR-165), or else until
//! a run finds it closed.

use std::time::Duration;
use tokio::time::Instant;

use super::{RETRY_AFTER, SessionKey, Slot, State, Unavailable, Version, release};

/// A session that lived less than this was a failed start.
pub const QUICK_EXIT: Duration = RETRY_AFTER;

/// Why an instance whose session lived less than `QUICK_EXIT` is not started
/// again at once.
pub const EXITED_AT_ONCE: &str = "Le serveur a cessé de répondre moins d’une minute après son \
     démarrage : il n’est pas redémarré avant une minute";

/// Releases the live instance of this version if its session closed for
/// good, to be started again; remembered as a failed start, and told so,
/// if it lived less than `QUICK_EXIT`.
pub(super) fn closed(state: &mut State, id: SessionKey, version: Version) -> Option<Unavailable> {
    let lived = match state.slots.get(&id) {
        Some(Slot::Live {
            version: v,
            runtime,
            since,
            ..
        }) if *v == version && runtime.is_closed() => match runtime.ended_at() {
            Some(ended) => ended.saturating_duration_since(*since),
            None => since.elapsed(),
        },
        _ => return None,
    };
    release(state.slots.remove(&id));
    if lived >= QUICK_EXIT {
        let connector = id.instance;
        tracing::warn!(%connector, "A kept connector's session closed: starting it again");
        return None;
    }
    let failed = Slot::Failed {
        version,
        until: Instant::now() + RETRY_AFTER,
        message: EXITED_AT_ONCE.into(),
    };
    state.slots.insert(id, failed);
    Some(Unavailable::new(EXITED_AT_ONCE))
}

#[cfg(test)]
mod tests {
    //! A session closed for good is started again, unless it closed soon
    //! after its start, judged by when it ended once it says so.

    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use chrono::Utc;
    use mimorya_connectors::ConnectorRuntime;
    use uuid::Uuid;

    use super::super::tests::{Started, unavailable};
    use super::super::{Origin, RuntimeCache};
    use super::*;

    /// A session that can be told it closed, and records being released.
    #[derive(Default)]
    struct Session {
        closed: AtomicBool,
        released: AtomicBool,
        /// When it ended, if it says so (ADR-165).
        ended: std::sync::Mutex<Option<Instant>>,
    }

    /// The closer an instance holds, sharing the session with the test.
    struct Closer(Arc<Session>);

    #[async_trait::async_trait]
    impl mimorya_connectors::ConnectorCloser for Closer {
        async fn close(&self) {
            self.0.released.store(true, Ordering::SeqCst);
        }

        fn is_closed(&self) -> bool {
            self.0.closed.load(Ordering::SeqCst)
        }

        fn ended_at(&self) -> Option<Instant> {
            *self.0.ended.lock().unwrap()
        }
    }

    /// A start that counts itself and gives an instance of `session`.
    fn with(
        starts: &Arc<AtomicUsize>,
        session: &Arc<Session>,
    ) -> impl FnOnce() -> Started + Send + 'static {
        let (starts, session) = (starts.clone(), session.clone());
        move || -> Started {
            Box::pin(async move {
                starts.fetch_add(1, Ordering::SeqCst);
                Ok(ConnectorRuntime {
                    tools: Vec::new(),
                    closer: Some(Box::new(Closer(session))),
                })
            })
        }
    }

    /// Returns once the session was released, in the background. Counted
    /// in turns given to the other tasks, not in time: a paused clock does
    /// not advance while this test runs.
    async fn released(session: &Session) {
        for _ in 0..10_000 {
            if session.released.load(Ordering::SeqCst) {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("the closed instance is never released");
    }

    #[tokio::test(start_paused = true)]
    async fn starts_again_an_instance_whose_session_closed_for_good() {
        let cache = RuntimeCache::default();
        let (id, version) = (Uuid::new_v4(), Utc::now());
        let starts = Arc::new(AtomicUsize::new(0));
        let (first, second) = (Arc::new(Session::default()), Arc::new(Session::default()));
        let Ok(live) = cache
            .get_or_start(id, version, 0, None, with(&starts, &first))
            .await
        else {
            panic!("expected a live instance");
        };
        // Still open: kept.
        let kept = cache.get_or_start(id, version, 0, None, with(&starts, &second));
        let Ok(kept) = kept.await else {
            panic!("expected the live instance");
        };
        assert!(Arc::ptr_eq(&live, &kept));
        assert_eq!(starts.load(Ordering::SeqCst), 1);

        // Closed for good, long after its start: released, and started again.
        tokio::time::advance(QUICK_EXIT).await;
        first.closed.store(true, Ordering::SeqCst);
        let again = cache.get_or_start(id, version, 0, None, with(&starts, &second));
        let Ok(again) = again.await else {
            panic!("expected a new instance");
        };
        assert!(!Arc::ptr_eq(&live, &again));
        assert_eq!(starts.load(Ordering::SeqCst), 2);
        released(&first).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_session_closed_soon_after_its_start_is_a_failed_start() {
        let cache = RuntimeCache::default();
        let (id, version) = (Uuid::new_v4(), Utc::now());
        let starts = Arc::new(AtomicUsize::new(0));
        let session = Arc::new(Session::default());
        let start = || with(&starts, &session);
        let Ok(_) = cache.get_or_start(id, version, 0, None, start()).await else {
            panic!("expected a live instance");
        };
        // A server that exits as soon as started: told once, then remembered.
        tokio::time::advance(QUICK_EXIT - std::time::Duration::from_secs(1)).await;
        session.closed.store(true, Ordering::SeqCst);
        let exited = unavailable(cache.get_or_start(id, version, 0, None, start()).await);
        assert_eq!(exited.origin, Origin::Fresh);
        assert_eq!(exited.message, EXITED_AT_ONCE);
        let remembered = unavailable(cache.get_or_start(id, version, 0, None, start()).await);
        assert_eq!(remembered.origin, Origin::Remembered);
        assert_eq!(
            starts.load(Ordering::SeqCst),
            1,
            "not started again at once"
        );
        released(&session).await;
        // Tried again once the retry delay is over.
        session.closed.store(false, Ordering::SeqCst);
        tokio::time::advance(RETRY_AFTER).await;
        let Ok(_) = cache.get_or_start(id, version, 0, None, start()).await else {
            panic!("expected a new instance");
        };
        assert_eq!(starts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn a_session_is_judged_by_when_it_ended_once_dated() {
        // Ended a second after its start, or a minute after: found long after.
        for (lived, failed) in [
            (std::time::Duration::from_secs(1), true),
            (QUICK_EXIT, false),
        ] {
            let cache = RuntimeCache::default();
            let (id, version) = (Uuid::new_v4(), Utc::now());
            let starts = Arc::new(AtomicUsize::new(0));
            let session = Arc::new(Session::default());
            let start = || with(&starts, &session);
            let Ok(_) = cache.get_or_start(id, version, 0, None, start()).await else {
                panic!("expected a live instance");
            };
            tokio::time::advance(lived).await;
            *session.ended.lock().unwrap() = Some(Instant::now());
            session.closed.store(true, Ordering::SeqCst);
            tokio::time::advance(QUICK_EXIT * 2).await;
            // Started again, if it is, with a session of its own.
            let fresh = Arc::new(Session::default());
            let again = with(&starts, &fresh);
            let found = cache.get_or_start(id, version, 0, None, again).await;
            released(&session).await;
            if failed {
                assert_eq!(unavailable(found).message, EXITED_AT_ONCE);
                assert_eq!(starts.load(Ordering::SeqCst), 1, "not started again");
            } else {
                assert!(found.is_ok());
                assert_eq!(starts.load(Ordering::SeqCst), 2, "started again");
            }
        }
    }
}
