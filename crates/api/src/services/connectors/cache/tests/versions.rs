//! Versions of an instance: a start invalidated while it ran is not kept,
//! a run that read a changed instance does not start it, and a newer
//! instance, live, starting or failed, serves a run that read an older one
//! rather than it start the older in its place, while its reading is not
//! stale and the failure remembered; an older start ending late never
//! takes a newer one's place, unless that one's failure is forgotten
//! (ADR-177).

use super::*;

#[tokio::test]
async fn never_keeps_a_start_invalidated_while_it_ran() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let ticket = cache.ticket().await;
    let invalidating = cache.clone();
    let start = move || -> Started {
        Box::pin(async move {
            invalidating.invalidate(id).await;
            Ok(runtime())
        })
    };
    let invalidated = unavailable(cache.get_or_start(id, version, ticket, None, start).await);
    assert_eq!(invalidated.origin, Origin::Changed);
    // Nothing was kept: the next run (a later ticket) starts again.
    let starts = Arc::new(AtomicUsize::new(0));
    let ticket = cache.ticket().await;
    let working = counted(&starts, || Ok(runtime()));
    let restarted = cache.get_or_start(id, version, ticket, None, working).await;
    assert!(restarted.is_ok());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// A run that read an instance since changed does not start it, but gets
/// the live instance of the version it read.
#[tokio::test]
async fn a_run_that_read_a_changed_instance_does_not_start_it() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let ticket = cache.ticket().await;
    cache.invalidate(id).await;
    let starts = Arc::new(AtomicUsize::new(0));
    let start = || counted(&starts, || Ok(runtime()));
    let changed = unavailable(cache.get_or_start(id, version, ticket, None, start()).await);
    assert_eq!(changed.origin, Origin::Changed);
    assert_eq!(starts.load(Ordering::SeqCst), 0);

    // Started meanwhile for a run with a later reading: shared all the same.
    let later = cache.ticket().await;
    let Ok(live) = cache.get_or_start(id, version, later, None, start()).await else {
        panic!("expected a live instance");
    };
    let Ok(shared) = cache.get_or_start(id, version, ticket, None, start()).await else {
        panic!("expected the live instance");
    };
    assert!(Arc::ptr_eq(&live, &shared));
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// A run that read an older version than the one live, its reading not
/// stale (a person's authorization renewed meanwhile), gets the newer one
/// rather than start the older in its place.
#[tokio::test]
async fn a_newer_live_instance_serves_a_run_that_read_an_older_one() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let released = Arc::new(AtomicBool::new(false));
    let live = flagged(&released);
    let starting = counted(&starts, move || Ok(live));
    let Ok(live) = cache.get_or_start(id, newer, 0, None, starting).await else {
        panic!("expected a live instance");
    };
    let older_start = counted(&starts, || Ok(runtime()));
    let Ok(served) = cache.get_or_start(id, older, 0, None, older_start).await else {
        panic!("expected the newer instance");
    };
    assert!(Arc::ptr_eq(&live, &served));
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert!(!released.load(Ordering::SeqCst));
}

/// A run that read an older version while a newer one starts, its reading
/// not stale, waits for that start rather than start the older in its
/// place; served the newer instance.
#[tokio::test(start_paused = true)]
async fn a_newer_start_serves_a_run_that_read_an_older_one() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let patience = Some(Duration::from_millis(20));
    let newer_start = gated(&starts, &gate, || Ok(runtime()));
    let waited = within(cache.get_or_start(id, newer, 0, patience, newer_start)).await;
    assert_eq!(unavailable(waited).origin, Origin::Starting);
    let older_start = counted(&starts, || Ok(runtime()));
    let (served, ()) = within(async {
        tokio::join!(cache.get_or_start(id, older, 0, None, older_start), async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            gate.notify_one();
        })
    })
    .await;
    let Ok(served) = served else {
        panic!("expected the newer instance");
    };
    let again = counted(&starts, || Ok(runtime()));
    let Ok(live) = cache.get_or_start(id, newer, 0, None, again).await else {
        panic!("expected the newer instance, live");
    };
    assert!(Arc::ptr_eq(&served, &live));
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// A run that read an older version while a newer one starts, which then
/// fails, is told that failure, remembered: it neither starts the older in
/// its place nor makes the newer version's runs start it again at once.
#[tokio::test(start_paused = true)]
async fn a_newer_failure_serves_a_run_that_read_an_older_one() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let patience = Some(Duration::from_millis(20));
    let newer_start = gated(&starts, &gate, failing);
    let waited = within(cache.get_or_start(id, newer, 0, patience, newer_start)).await;
    assert_eq!(unavailable(waited).origin, Origin::Starting);
    let older_start = counted(&starts, || Ok(runtime()));
    let (told, ()) = within(async {
        tokio::join!(cache.get_or_start(id, older, 0, None, older_start), async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            gate.notify_one();
        })
    })
    .await;
    let told = unavailable(told);
    assert_eq!(told.origin, Origin::Remembered);
    assert_eq!(told.message, "injoignable");
    let again = counted(&starts, || Ok(runtime()));
    let remembered = unavailable(cache.get_or_start(id, newer, 0, None, again).await);
    assert_eq!(remembered.origin, Origin::Remembered);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// An older start that ends while a newer one runs leaves it in place:
/// its instance released, the runs of the newer version wait for theirs.
#[tokio::test(start_paused = true)]
async fn an_older_start_ending_late_leaves_a_newer_start_in_place() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let (older_gate, newer_gate) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let patience = Some(Duration::from_millis(20));
    let released = Arc::new(AtomicBool::new(false));
    let older_live = flagged(&released);
    let older_start = gated(&starts, &older_gate, move || Ok(older_live));
    let waited = within(cache.get_or_start(id, older, 0, patience, older_start)).await;
    assert_eq!(unavailable(waited).origin, Origin::Starting);
    let newer_start = gated(&starts, &newer_gate, || Ok(runtime()));
    let waited = within(cache.get_or_start(id, newer, 0, patience, newer_start)).await;
    assert_eq!(unavailable(waited).origin, Origin::Starting);
    older_gate.notify_one();
    assert!(raised(&released).await, "the older instance is released");
    let again = counted(&starts, || Ok(runtime()));
    let (served, ()) = within(async {
        tokio::join!(cache.get_or_start(id, newer, 0, None, again), async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            newer_gate.notify_one();
        })
    })
    .await;
    assert!(served.is_ok());
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

/// An older start that ends once a newer one failed leaves that failure
/// remembered: the newer version's runs do not start it again at once.
#[tokio::test(start_paused = true)]
async fn an_older_start_ending_late_leaves_a_newer_failure_remembered() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let patience = Some(Duration::from_millis(20));
    let released = Arc::new(AtomicBool::new(false));
    let older_live = flagged(&released);
    let older_start = gated(&starts, &gate, move || Ok(older_live));
    let waited = within(cache.get_or_start(id, older, 0, patience, older_start)).await;
    assert_eq!(unavailable(waited).origin, Origin::Starting);
    let newer_start = counted(&starts, failing);
    let failed = within(cache.get_or_start(id, newer, 0, None, newer_start)).await;
    assert_eq!(unavailable(failed).message, "injoignable");
    gate.notify_one();
    assert!(raised(&released).await, "the older instance is released");
    let again = counted(&starts, || Ok(runtime()));
    let remembered = unavailable(cache.get_or_start(id, newer, 0, None, again).await);
    assert_eq!(remembered.origin, Origin::Remembered);
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

/// A newer failure serves a run that read an older version only while the
/// run's reading is not stale, and while the failure is remembered.
#[tokio::test(start_paused = true)]
async fn a_newer_failure_serves_only_a_current_reading_while_remembered() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let stale = cache.ticket().await;
    cache.invalidate(id).await;
    let current = cache.ticket().await;
    let failing_start = counted(&starts, failing);
    let failed = cache
        .get_or_start(id, newer, current, None, failing_start)
        .await;
    assert_eq!(unavailable(failed).origin, Origin::Remembered);
    // A stale reading of an older version: changed, nothing started.
    let read = counted(&starts, || Ok(runtime()));
    let changed = unavailable(cache.get_or_start(id, older, stale, None, read).await);
    assert_eq!(changed.origin, Origin::Changed);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    // The failure forgotten, a current reading of the older version starts.
    tokio::time::advance(RETRY_AFTER).await;
    let read = counted(&starts, || Ok(runtime()));
    let started = cache.get_or_start(id, older, current, None, read).await;
    assert!(started.is_ok());
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

/// An older start that ends once a newer failure is forgotten keeps its
/// instance: the run that waited for it gets it, nothing started again.
#[tokio::test(start_paused = true)]
async fn an_older_start_ending_after_a_newer_failure_forgotten_keeps_its_instance() {
    let cache = RuntimeCache::default();
    let id = Uuid::new_v4();
    let (older, newer) = (Utc::now(), Utc::now() + chrono::Duration::seconds(1));
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let older_start = gated(&starts, &gate, || Ok(runtime()));
    // Not bounded by `within`: the clock jumps past the retry delay.
    let (waited, ()) = tokio::join!(cache.get_or_start(id, older, 0, None, older_start), async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let newer_start = counted(&starts, failing);
        let failed = cache.get_or_start(id, newer, 0, None, newer_start).await;
        assert_eq!(unavailable(failed).message, "injoignable");
        tokio::time::advance(RETRY_AFTER).await;
        gate.notify_one();
    });
    assert!(waited.is_ok(), "the older start's instance serves its run");
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}
