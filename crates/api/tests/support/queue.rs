//! Processes of each role (ADR-085): an `api` process, which queues the runs
//! it starts, and a worker, which claims and executes them.

use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

use super::{Client, TestApi, owner_url};

pub const API: &[(&str, &str)] = &[("MIMORYA_ROLE", "api")];
pub const WORKER: &[(&str, &str)] = &[("MIMORYA_ROLE", "worker")];

/// A worker holding the run executor's lock, ready to take queued runs.
pub async fn worker() -> TestApi {
    let worker = TestApi::start_with(WORKER).await;
    worker.app.services.runs.claim_execution().await.unwrap();
    worker
}

/// Waits until `process` hears what the others change in the caches.
pub async fn listening(process: &TestApi) {
    for _ in 0..500 {
        if process.app.services.invalidations.listening() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("the process does not hear the others' invalidations");
}

/// Takes the queued runs, then waits for their execution; how many.
pub async fn take_queued(worker: &TestApi) -> usize {
    let services = &worker.app.services;
    let taken = services.runs.take_queued(&services.shutdown).await.unwrap();
    worker.idle().await;
    taken
}

/// Sends `text` in a new conversation of `client`'s: the conversation and
/// its run.
pub async fn ask(client: &Client<'_>, text: &str) -> (Uuid, Uuid) {
    let opened = client.post("/conversations", json!({})).await.json();
    let conversation: Uuid = opened["conversation"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let sent = client
        .post(
            &format!("/conversations/{conversation}/messages"),
            json!({ "content": text }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    let run = sent.json()["runId"].as_str().unwrap().parse().unwrap();
    (conversation, run)
}

/// The run's status, as the owner reads it.
pub async fn status(run: Uuid) -> String {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let status: String = sqlx::query_scalar("select status from agent_runs where id = $1")
        .bind(run)
        .fetch_one(&owner)
        .await
        .unwrap();
    owner.close().await;
    status
}

/// Moves back when `run` was queued by `minutes`.
pub async fn queued_ago(run: Uuid, minutes: i64) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(
        "update agent_runs set updated_at = now() - make_interval(mins => $2::int) \
         where id = $1",
    )
    .bind(run)
    .bind(i32::try_from(minutes).unwrap())
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

/// Runs `sql` as the owner, `$1` bound to `id`: what an operator, or an
/// administrator through another process, changes meanwhile.
pub async fn as_owner(sql: &str, id: Uuid) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(sql).bind(id).execute(&owner).await.unwrap();
    owner.close().await;
}
