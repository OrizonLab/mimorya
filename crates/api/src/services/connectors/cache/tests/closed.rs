//! A session closed for good is started again, unless it closed soon
//! after its start, judged by when it ended once it says so (`closed`).
//! The newer instances closed so: `closed_versions`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use chrono::Utc;
use mimorya_connectors::ConnectorRuntime;
use tokio::time::Instant;
use uuid::Uuid;

use super::super::closed::{EXITED_AT_ONCE, QUICK_EXIT};
use super::super::{Origin, RETRY_AFTER, RuntimeCache};
use super::{Started, raised, unavailable};

/// A session that can be told it closed, and records being released.
#[derive(Default)]
pub(super) struct Session {
    pub(super) closed: AtomicBool,
    pub(super) released: AtomicBool,
    /// When it ended, if it says so (ADR-165).
    pub(super) ended: std::sync::Mutex<Option<Instant>>,
}

/// The closer an instance holds, sharing the session with the test.
pub(super) struct Closer(pub(super) Arc<Session>);

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
pub(super) fn with(
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

/// Returns once the session was released, in the background.
pub(super) async fn released(session: &Session) {
    let released = raised(&session.released).await;
    assert!(released, "the closed instance is never released");
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
