//! The cache's rules: live instances and failures per version, starts
//! invalidated while they ran, starts shared by the runs that need them,
//! and run in the background (ADR-177): waited for at most a run's
//! patience, going on when the run that asked for one is gone, their
//! failure remembered, a start that panics failed, one ending after the
//! cache closed released; a newer instance serves a run that read an older
//! one. The sessions closed for good are tested with `closed`.

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Notify;

use super::starting::INTERRUPTED;
use super::*;

pub(super) type Started = Pin<Box<dyn Future<Output = Result<ConnectorRuntime, String>> + Send>>;

pub(super) fn unavailable(result: Result<Arc<ConnectorRuntime>, Unavailable>) -> Unavailable {
    match result {
        Err(unavailable) => unavailable,
        Ok(_) => panic!("expected the instance to be unavailable"),
    }
}

pub(super) fn runtime() -> ConnectorRuntime {
    ConnectorRuntime {
        tools: Vec::new(),
        closer: None,
    }
}

/// A start that counts itself, then gives what `make` makes.
pub(super) fn counted(
    starts: &Arc<AtomicUsize>,
    make: impl FnOnce() -> Result<ConnectorRuntime, String> + Send + 'static,
) -> impl FnOnce() -> Started + Send + 'static {
    let starts = starts.clone();
    move || -> Started {
        Box::pin(async move {
            starts.fetch_add(1, Ordering::SeqCst);
            make()
        })
    }
}

/// A start that counts itself, then waits for `gate` before it gives what
/// `make` makes.
fn gated(
    starts: &Arc<AtomicUsize>,
    gate: &Arc<Notify>,
    make: impl FnOnce() -> Result<ConnectorRuntime, String> + Send + 'static,
) -> impl FnOnce() -> Started + Send + 'static {
    let (starts, gate) = (starts.clone(), gate.clone());
    move || -> Started {
        Box::pin(async move {
            starts.fetch_add(1, Ordering::SeqCst);
            gate.notified().await;
            make()
        })
    }
}

fn failing() -> Result<ConnectorRuntime, String> {
    Err("injoignable".to_string())
}

/// The closer of an instance, telling the test it was released.
struct Flag(Arc<AtomicBool>);

#[async_trait::async_trait]
impl mimorya_connectors::ConnectorCloser for Flag {
    async fn close(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// An instance whose release `released` tells.
fn flagged(released: &Arc<AtomicBool>) -> ConnectorRuntime {
    ConnectorRuntime {
        tools: Vec::new(),
        closer: Some(Box::new(Flag(released.clone()))),
    }
}

/// Bounded: a run that kept waiting would never end.
async fn within<T>(waited: impl Future<Output = T>) -> T {
    let Ok(got) = tokio::time::timeout(Duration::from_secs(5), waited).await else {
        panic!("a run kept waiting");
    };
    got
}

#[tokio::test]
async fn keeps_a_live_instance_and_remembers_a_failure_per_version() {
    let cache = RuntimeCache::default();
    let (id, v1, v2) = (
        Uuid::new_v4(),
        Utc::now(),
        Utc::now() + chrono::Duration::seconds(1),
    );
    let starts = Arc::new(AtomicUsize::new(0));
    let failing = || counted(&starts, failing);
    // Logged by its start: remembered for every run that reads it.
    let first = unavailable(cache.get_or_start(id, v1, 0, None, failing()).await);
    assert_eq!(first.origin, Origin::Remembered);
    let again = unavailable(cache.get_or_start(id, v1, 0, None, failing()).await);
    assert_eq!(again.origin, Origin::Remembered);
    assert_eq!(again.message, "injoignable");
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    // A tested instance is tried again at once.
    cache.retry_now(id).await;
    let retried = unavailable(cache.get_or_start(id, v1, 0, None, failing()).await);
    assert_eq!(retried.message, "injoignable");
    assert_eq!(starts.load(Ordering::SeqCst), 2);

    // A new version starts again, then stays live.
    let working = || counted(&starts, || Ok(runtime()));
    let Ok(live) = cache.get_or_start(id, v2, 0, None, working()).await else {
        panic!("expected a live instance");
    };
    let Ok(reused) = cache.get_or_start(id, v2, 0, None, working()).await else {
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

/// Two runs need the instance at once: the second waits for the first's start.
#[tokio::test(start_paused = true)]
async fn a_start_in_progress_is_shared_by_the_runs_that_need_it() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let start = || gated(&starts, &gate, || Ok(runtime()));
    let (first, second, ()) = within(async {
        tokio::join!(
            cache.get_or_start(id, version, 0, None, start()),
            async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                cache.get_or_start(id, version, 0, None, start()).await
            },
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                gate.notify_one();
            },
        )
    })
    .await;
    let (Ok(first), Ok(second)) = (first, second) else {
        panic!("expected the started instance");
    };
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// The failure of the start two runs waited for, logged by the start, is
/// remembered for both.
#[tokio::test(start_paused = true)]
async fn a_failed_start_in_progress_is_shared_as_remembered() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let start = || gated(&starts, &gate, failing);
    let (first, second, ()) = within(async {
        tokio::join!(
            cache.get_or_start(id, version, 0, None, start()),
            async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                cache.get_or_start(id, version, 0, None, start()).await
            },
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                gate.notify_one();
            },
        )
    })
    .await;
    let origins = [unavailable(first).origin, unavailable(second).origin];
    assert_eq!(origins, [Origin::Remembered, Origin::Remembered]);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// A run waits for a start at most its patience: it goes without the
/// instance, told it still starts, and the start serves the next run.
#[tokio::test(start_paused = true)]
async fn a_run_waits_for_a_start_no_longer_than_its_patience() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let patience = Some(Duration::from_millis(50));
    let start = || gated(&starts, &gate, || Ok(runtime()));
    let waited = within(cache.get_or_start(id, version, 0, patience, start())).await;
    let starting = unavailable(waited);
    assert_eq!(starting.origin, Origin::Starting);
    assert_eq!(starting.message, STILL_STARTING);
    // Another run, as patient, waits for the same start, which then ends.
    let (next, ()) = within(async {
        tokio::join!(
            cache.get_or_start(id, version, 0, patience, start()),
            async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                gate.notify_one();
            }
        )
    })
    .await;
    assert!(next.is_ok());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// A start goes on when the run that asked for it is gone: a run waiting
/// for it gets its outcome, nothing started again.
#[tokio::test(start_paused = true)]
async fn a_start_goes_on_when_the_run_that_asked_for_it_is_gone() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let start = || gated(&starts, &gate, || Ok(runtime()));
    let gone = tokio::time::timeout(
        Duration::from_millis(20),
        cache.get_or_start(id, version, 0, None, start()),
    )
    .await;
    assert!(gone.is_err(), "the run is gone before the start ends");
    let (waiting, ()) = within(async {
        tokio::join!(cache.get_or_start(id, version, 0, None, start()), async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            gate.notify_one();
        })
    })
    .await;
    assert!(waiting.is_ok());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// A failure found once no run waited any more is remembered for the next
/// runs, logged by the start that found it.
#[tokio::test(start_paused = true)]
async fn a_failure_found_in_the_background_is_remembered_for_the_next_runs() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let patience = Some(Duration::from_millis(20));
    let start = || gated(&starts, &gate, failing);
    let waited = within(cache.get_or_start(id, version, 0, patience, start())).await;
    assert_eq!(unavailable(waited).origin, Origin::Starting);
    gate.notify_one();
    let told = within(cache.get_or_start(id, version, 0, None, start())).await;
    let told = unavailable(told);
    assert_eq!(told.origin, Origin::Remembered);
    assert_eq!(told.message, "injoignable");
    let remembered = within(cache.get_or_start(id, version, 0, None, start())).await;
    assert_eq!(unavailable(remembered).origin, Origin::Remembered);
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

/// A start that panics is a failed one, remembered: not started again by
/// every run.
#[tokio::test]
async fn a_start_that_panics_is_a_failed_start() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let starts = Arc::new(AtomicUsize::new(0));
    let panicking = || counted(&starts, || panic!("démarrage interrompu par le test"));
    let failed = unavailable(cache.get_or_start(id, version, 0, None, panicking()).await);
    assert_eq!(failed.message, INTERRUPTED);
    let again = unavailable(cache.get_or_start(id, version, 0, None, panicking()).await);
    assert_eq!(again.origin, Origin::Remembered);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// A start ending once the cache closed (shutdown) releases what it
/// started rather than keep it.
#[tokio::test(start_paused = true)]
async fn a_start_ending_after_the_cache_closed_releases_its_instance() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let starts = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(Notify::new());
    let released = Arc::new(AtomicBool::new(false));
    let live = flagged(&released);
    let start = gated(&starts, &gate, move || Ok(live));
    let patience = Some(Duration::from_millis(20));
    let waited = within(cache.get_or_start(id, version, 0, patience, start)).await;
    assert_eq!(unavailable(waited).origin, Origin::Starting);
    cache.close().await;
    gate.notify_one();
    for _ in 0..10_000 {
        if released.load(Ordering::SeqCst) {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("the instance started after the cache closed is kept");
}
