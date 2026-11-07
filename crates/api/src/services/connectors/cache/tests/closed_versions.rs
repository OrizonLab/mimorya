//! Newer instances whose session closed for good (ADR-163, ADR-177): a run
//! that read an older version, or an older start ending late, judges one
//! before it serves or holds the place; a stale reading leaves it to the
//! runs of its own version.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::Utc;
use mimorya_connectors::ConnectorRuntime;
use uuid::Uuid;

use super::super::closed::{EXITED_AT_ONCE, QUICK_EXIT};
use super::super::{Origin, RuntimeCache};
use super::closed::{Closer, Session, released, with};
use super::{capture_logs, gated, unavailable};

/// A newer live instance about to serve a run that read an older
/// version is judged first: closed soon after its start, it is a failed
/// start of its own version, told as such, never given to the run.
#[tokio::test(start_paused = true)]
async fn a_newer_instance_closed_is_judged_before_it_serves_an_older_run() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let (session, other) = (Arc::new(Session::default()), Arc::new(Session::default()));
    let started = cache.get_or_start(id, newer, 0, None, with(&starts, &session));
    let Ok(_) = started.await else {
        panic!("expected a live instance");
    };
    session.closed.store(true, Ordering::SeqCst);
    let read_older = cache.get_or_start(id, older, 0, None, with(&starts, &other));
    let exited = unavailable(read_older.await);
    assert_eq!(exited.message, EXITED_AT_ONCE);
    // Remembered for the newer version's runs: not started again by them.
    let read_newer = cache.get_or_start(id, newer, 0, None, with(&starts, &other));
    assert_eq!(unavailable(read_newer.await).origin, Origin::Remembered);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    released(&session).await;
}

/// A newer live instance whose session closed long after its start is
/// released, never given to a run that read an older version: that run
/// starts the version it read, as it would have with no newer one.
#[tokio::test(start_paused = true)]
async fn a_newer_instance_closed_late_is_released_for_an_older_run() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let (session, other) = (Arc::new(Session::default()), Arc::new(Session::default()));
    let started = cache.get_or_start(id, newer, 0, None, with(&starts, &session));
    let Ok(closing) = started.await else {
        panic!("expected a live instance");
    };
    tokio::time::advance(QUICK_EXIT).await;
    session.closed.store(true, Ordering::SeqCst);
    let read_older = cache.get_or_start(id, older, 0, None, with(&starts, &other));
    let Ok(own) = read_older.await else {
        panic!("expected the older version started");
    };
    assert!(!Arc::ptr_eq(&closing, &own));
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    released(&session).await;
}

/// A start of an older version ending once a newer instance's session
/// closed for good does not give way to it: that instance is released,
/// the older start's kept for the run that waited for it.
#[tokio::test(start_paused = true)]
async fn an_older_start_ending_late_judges_a_newer_instance_first() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let session = Arc::new(Session::default());
    let gate = Arc::new(tokio::sync::Notify::new());
    let older_start = gated(&starts, &gate, || {
        Ok(ConnectorRuntime {
            tools: Vec::new(),
            closer: None,
        })
    });
    // Bounded beyond the clock's jump past `QUICK_EXIT`.
    let bound = QUICK_EXIT + std::time::Duration::from_secs(5);
    let ended = tokio::time::timeout(bound, async {
        tokio::join!(cache.get_or_start(id, older, 0, None, older_start), async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let started = cache.get_or_start(id, newer, 0, None, with(&starts, &session));
            assert!(started.await.is_ok(), "the newer instance went live");
            tokio::time::advance(QUICK_EXIT).await;
            session.closed.store(true, Ordering::SeqCst);
            gate.notify_one();
        })
    });
    let Ok((waited, ())) = ended.await else {
        panic!("a run kept waiting");
    };
    assert!(waited.is_ok(), "the older start's instance serves its run");
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    released(&session).await;
}

/// The same, the newer instance's session closed soon after its start:
/// it is replaced by its failure, remembered and logged by the start that
/// found it, which holds the place; the older start's instance is
/// released, and the run that waited for it told that failure.
#[tokio::test(start_paused = true)]
async fn an_older_start_ending_late_finds_a_newer_instance_closed_at_once() {
    let logs = capture_logs();
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let (session, own) = (Arc::new(Session::default()), Arc::new(Session::default()));
    let gate = Arc::new(tokio::sync::Notify::new());
    let kept = own.clone();
    let older_start = gated(&starts, &gate, move || {
        Ok(ConnectorRuntime {
            tools: Vec::new(),
            closer: Some(Box::new(Closer(kept))),
        })
    });
    let ended = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(cache.get_or_start(id, older, 0, None, older_start), async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let started = cache.get_or_start(id, newer, 0, None, with(&starts, &session));
            assert!(started.await.is_ok(), "the newer instance went live");
            session.closed.store(true, Ordering::SeqCst);
            gate.notify_one();
        })
    });
    let Ok((waited, ())) = ended.await else {
        panic!("a run kept waiting");
    };
    let told = unavailable(waited);
    assert_eq!(told.message, EXITED_AT_ONCE);
    assert_eq!(told.origin, Origin::Remembered);
    released(&own).await;
    released(&session).await;
    let other = Arc::new(Session::default());
    let read_newer = cache.get_or_start(id, newer, 0, None, with(&starts, &other));
    assert_eq!(unavailable(read_newer.await).origin, Origin::Remembered);
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    // Logged once, by the start, naming the connector and why.
    let logged = logs.text();
    assert_eq!(
        logged.matches("Connector unavailable").count(),
        1,
        "{logged}"
    );
    assert!(logged.contains(&id.to_string()), "{logged}");
    assert!(logged.contains(EXITED_AT_ONCE), "{logged}");
}

/// A stale reading leaves a newer instance whose session closed at once
/// to the runs of its own version: neither a stale start nor a stale
/// run judges it, and the first current run finds it, as a failure of
/// its own (logged and recorded on the connector).
#[tokio::test(start_paused = true)]
async fn a_stale_reading_leaves_a_newer_instance_closed_to_its_own_runs() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let session = Arc::new(Session::default());
    let gate = Arc::new(tokio::sync::Notify::new());
    let older_start = gated(&starts, &gate, || {
        Ok(ConnectorRuntime {
            tools: Vec::new(),
            closer: None,
        })
    });
    let stale = cache.ticket().await;
    let ended = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(
            cache.get_or_start(id, older, stale, None, older_start),
            async {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                cache.invalidate(id).await;
                let current = cache.ticket().await;
                let started = cache.get_or_start(id, newer, current, None, with(&starts, &session));
                assert!(started.await.is_ok(), "the newer instance went live");
                session.closed.store(true, Ordering::SeqCst);
                gate.notify_one();
            }
        )
    });
    let Ok((waited, ())) = ended.await else {
        panic!("a run kept waiting");
    };
    assert_eq!(unavailable(waited).origin, Origin::Changed);
    let current = cache.ticket().await;
    let other = Arc::new(Session::default());
    let read_newer = cache.get_or_start(id, newer, current, None, with(&starts, &other));
    let found = unavailable(read_newer.await);
    assert_eq!(
        (found.origin, found.message.as_str()),
        (Origin::Fresh, EXITED_AT_ONCE)
    );
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    released(&session).await;
}
