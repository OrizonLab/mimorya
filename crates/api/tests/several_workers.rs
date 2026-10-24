//! Several workers side by side (ADR-086): a sweep that runs once at a time
//! holds a lock no other process gets meanwhile, released at its end, or
//! with its connection when its process vanishes; another sweep's lock
//! stays free, and a process that waits for one gets it once released.
//! Queued runs are shared out between workers, each claimed once.

mod support;

use std::time::Duration;

use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::user;
use support::TestApi;
use support::queue::{API, ask, status, take_queued, worker};

/// The connections holding the sweep `name`'s lock, as an operator finds
/// them.
async fn holders(name: &str) -> i64 {
    let owner = sqlx::PgPool::connect(support::owner_url()).await.unwrap();
    let count = sqlx::query_scalar(
        "select count(*) from pg_stat_activity \
         where application_name = $1 and datname = current_database()",
    )
    .bind(format!("mimorya-sweep-{name}"))
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    count
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sweep_s_lock_is_held_by_one_process_at_a_time() {
    let (one, two) = (TestApi::start().await, TestApi::start().await);
    let (db_one, db_two) = (&one.app.services.db, two.app.services.db.clone());
    let held = db_one.try_sweep_lock("sweep-test").await.unwrap();
    let held = held.expect("free");
    assert!(db_two.try_sweep_lock("sweep-test").await.unwrap().is_none());
    assert_eq!(holders("sweep-test").await, 1);
    // Another sweep's lock is free meanwhile.
    let other = db_two.try_sweep_lock("other-sweep-test").await.unwrap();
    other.expect("another sweep").release().await;

    held.release().await;
    let again = db_two.try_sweep_lock("sweep-test").await.unwrap();
    let again = again.expect("released");

    // Waited for: got once released.
    let waiting = tokio::spawn(async move { db_two.sweep_lock("sweep-test").await.unwrap() });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!waiting.is_finished());
    again.release().await;
    let got = tokio::time::timeout(Duration::from_secs(5), waiting).await;
    got.expect("got it in time").unwrap().release().await;
    assert_eq!(holders("sweep-test").await, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sweep_s_lock_goes_with_a_process_that_vanished() {
    let (one, two) = (TestApi::start().await, TestApi::start().await);
    let held = one.app.services.db.try_sweep_lock("vanished-test").await;
    let held = held.unwrap().expect("free");
    let db = &two.app.services.db;
    assert!(db.try_sweep_lock("vanished-test").await.unwrap().is_none());

    // Its connection cut, as when its host vanishes.
    let app = sqlx::PgPool::connect(support::app_url()).await.unwrap();
    sqlx::query(
        "select pg_terminate_backend(pid) from pg_stat_activity \
         where application_name = 'mimorya-sweep-vanished-test' \
         and datname = current_database()",
    )
    .execute(&app)
    .await
    .unwrap();
    app.close().await;
    let mut free = None;
    for _ in 0..50 {
        free = db.try_sweep_lock("vanished-test").await.unwrap();
        if free.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    free.expect("released with its connection").release().await;
    drop(held);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn two_workers_share_the_queue_out_each_run_claimed_once() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let mut runs = Vec::new();
    for request in ["Point du lundi", "Point du mardi", "Point du mercredi"] {
        runs.push(ask(&marc, request).await.1);
    }
    let (one, two) = (worker().await, worker().await);
    for each in [&one, &two] {
        each.script((0..3).map(|_| ScriptedStep::text("Fait.")).collect());
    }

    let (first, second) = tokio::join!(take_queued(&one), take_queued(&two));
    assert_eq!(first + second, runs.len());
    for run in runs {
        assert_eq!(status(run).await, "completed");
    }
    let asked = |api: &TestApi| api.model.as_ref().unwrap().requests().len();
    assert_eq!(asked(&one) + asked(&two), 3);
}
