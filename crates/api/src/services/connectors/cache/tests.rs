//! The cache's rules: live instances and failures per version, starts
//! invalidated while they ran, starts shared by the runs that need them.

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
    assert_eq!(first.origin, Origin::Fresh);
    let again = unavailable(cache.get_or_start(id, v1, 0, failing).await);
    assert_eq!(again.origin, Origin::Remembered);
    assert_eq!(again.message, "injoignable");
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    // A tested instance is tried again at once.
    cache.retry_now(id).await;
    let retried = unavailable(cache.get_or_start(id, v1, 0, failing).await);
    assert_eq!(retried.origin, Origin::Fresh);
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
    assert_eq!(invalidated.origin, Origin::Changed);
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

/// Two runs need the instance at once: the second waits for the first's start.
#[tokio::test]
async fn a_start_in_progress_is_shared_by_the_runs_that_need_it() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let starts = AtomicUsize::new(0);
    let gate = tokio::sync::Notify::new();
    let start = || async {
        starts.fetch_add(1, Ordering::SeqCst);
        gate.notified().await;
        Ok(runtime())
    };
    // Bounded: a second start would wait for the gate forever.
    let both = async {
        tokio::join!(
            cache.get_or_start(id, version, 0, start),
            async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                cache.get_or_start(id, version, 0, start).await
            },
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                gate.notify_waiters();
            },
        )
    };
    let Ok((first, second, ())) = tokio::time::timeout(Duration::from_secs(5), both).await else {
        panic!("a run kept waiting");
    };
    let (Ok(first), Ok(second)) = (first, second) else {
        panic!("expected the started instance");
    };
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// The failure of the start a run waited for is told once, then remembered.
#[tokio::test]
async fn a_failed_start_in_progress_is_shared_as_remembered() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let starts = AtomicUsize::new(0);
    let start = || async {
        starts.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(50)).await;
        Err::<ConnectorRuntime, _>("injoignable".to_string())
    };
    let both = async {
        tokio::join!(cache.get_or_start(id, version, 0, start), async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cache.get_or_start(id, version, 0, start).await
        })
    };
    let Ok((first, second)) = tokio::time::timeout(Duration::from_secs(5), both).await else {
        panic!("a run kept waiting");
    };
    assert_eq!(unavailable(first).origin, Origin::Fresh);
    let second = unavailable(second);
    assert_eq!(second.origin, Origin::Remembered);
    assert_eq!(second.message, "injoignable");
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// The run making a start gives it up: a run waiting for it starts instead.
#[tokio::test]
async fn a_start_given_up_is_taken_over_by_a_run_waiting_for_it() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let starts = AtomicUsize::new(0);
    let never = || async {
        starts.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<Result<ConnectorRuntime, String>>().await
    };
    let working = || async {
        starts.fetch_add(1, Ordering::SeqCst);
        Ok(runtime())
    };
    let given_up = tokio::time::timeout(
        Duration::from_millis(50),
        cache.get_or_start(id, version, 0, never),
    );
    // Bounded: a run that kept waiting for a start given up would never end.
    let (given_up, taken_over) = tokio::join!(given_up, async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let waiting = cache.get_or_start(id, version, 0, working);
        tokio::time::timeout(Duration::from_secs(5), waiting).await
    });
    assert!(given_up.is_err());
    assert!(matches!(taken_over, Ok(Ok(_))));
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

/// A run that read an instance since changed does not start it, but gets
/// the live instance of the version it read.
#[tokio::test]
async fn a_run_that_read_a_changed_instance_does_not_start_it() {
    let cache = RuntimeCache::default();
    let (id, version) = (Uuid::new_v4(), Utc::now());
    let ticket = cache.ticket().await;
    cache.invalidate(id).await;
    let starts = AtomicUsize::new(0);
    let changed = unavailable(
        cache
            .get_or_start(id, version, ticket, || async {
                starts.fetch_add(1, Ordering::SeqCst);
                Ok(runtime())
            })
            .await,
    );
    assert_eq!(changed.origin, Origin::Changed);
    assert_eq!(starts.load(Ordering::SeqCst), 0);

    // Started meanwhile for a run with a later reading: shared all the same.
    let Ok(live) = cache
        .get_or_start(id, version, cache.ticket().await, || async {
            Ok(runtime())
        })
        .await
    else {
        panic!("expected a live instance");
    };
    let Ok(shared) = cache
        .get_or_start(id, version, ticket, || async {
            starts.fetch_add(1, Ordering::SeqCst);
            Ok(runtime())
        })
        .await
    else {
        panic!("expected the live instance");
    };
    assert!(Arc::ptr_eq(&live, &shared));
    assert_eq!(starts.load(Ordering::SeqCst), 0);
}
