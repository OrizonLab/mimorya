//! The server's passes over the scheduled tasks due (ADR-080): none before
//! the runs a stop interrupted are taken up, then at once; batch after batch
//! while tasks remain due, leaving those being changed to the next pass;
//! none once the server stops, which a pass does not hold up.

mod support;

use axum::http::StatusCode;
use chrono::Duration;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::user;
use serde_json::Value;
use support::scheduled::{
    TASKS, delete_tasks_of, due, every_day, fire, insert_late, listed, schedule,
};
use support::{TestApi, owner_url};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_server_fires_tasks_once_interrupted_runs_are_taken_up() {
    let api = TestApi::start_scheduling().await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("Au démarrage")).await;
    api.script(vec![ScriptedStep::text("Point fait.")]);
    due(id, Duration::minutes(1)).await;

    // Up, the runs a stop interrupted not yet taken up: a run started now
    // would be taken for one of them. Nothing fires.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(listed(&marc, id).await["lastOccurrence"], Value::Null);

    // Taken up: its first pass, at once.
    api.app.recover_interrupted_runs().await.unwrap();
    let mut outcome = Value::Null;
    for _ in 0..50 {
        outcome = listed(&marc, id).await["lastOccurrence"]["outcome"].clone();
        if !outcome.is_null() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(outcome, "started");
    api.idle().await;

    // Stopping, it ends between two passes: the server stops.
    let TestApi { app, .. } = api;
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(10), app.shutdown()).await;
    assert!(
        stopped.is_ok(),
        "the scheduler kept the server from stopping"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_stopping_server_leaves_due_tasks_to_its_next_start() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("À l’arrêt")).await;
    due(id, Duration::minutes(1)).await;
    api.app.begin_shutdown();
    assert_eq!(fire(&api).await, 0);
    let task = listed(&marc, id).await;
    assert_eq!(task["lastOccurrence"], Value::Null);
    let deleted = marc.delete(&format!("{TASKS}/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pass_goes_on_while_tasks_remain_due() {
    let api = TestApi::start().await;
    // More than a batch, all due two hours ago: none runs, each is
    // recorded missed in the one pass.
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    insert_late(&owner, user::NADIA, 120).await;
    assert_eq!(fire(&api).await, 0);
    let (missed, due): (i64, i64) = sqlx::query_as(
        "select count(*) filter (where last_outcome = 'missed'), \
         count(*) filter (where next_run_at <= now()) \
         from scheduled_tasks where user_id = $1",
    )
    .bind(user::NADIA)
    .fetch_one(&owner)
    .await
    .unwrap();
    delete_tasks_of(&owner, user::NADIA).await;
    owner.close().await;
    assert_eq!((missed, due), (120, 0));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pass_leaves_tasks_being_changed_to_the_next_one() {
    let api = TestApi::start().await;
    // A full batch due, every row held by changes under way: the pass ends
    // at once, without reading them again and again.
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    insert_late(&owner, user::NADIA, 50).await;
    let mut changes = owner.begin().await.unwrap();
    sqlx::query("select id from scheduled_tasks where user_id = $1 for update")
        .bind(user::NADIA)
        .execute(&mut *changes)
        .await
        .unwrap();
    let pass = tokio::time::timeout(std::time::Duration::from_secs(5), fire(&api)).await;
    changes.rollback().await.unwrap();
    delete_tasks_of(&owner, user::NADIA).await;
    owner.close().await;
    assert_eq!(pass.ok(), Some(0), "the pass kept reading the locked tasks");
}
