//! Stopping a mission (ADR-089): its stage under way is cancelled, queued,
//! awaiting a decision, or running, its requests with it; a stage running
//! writes and calls nothing more once stopped. A stage that ends while the
//! mission is being stopped is waited for, and the next one it queued is
//! the one cancelled. A mission over, or whose last stage failed, cannot be
//! stopped.

mod support;

use std::sync::{Mutex, mpsc};

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call};
use mimorya_api::plugins::maintenance::sweep_stale_runs;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::missions::{entrust, executing, messages, read, stages, stop};
use support::planning::{apply_proposal, propose};
use support::queue::take_queued;
use support::recovery::events;
use support::{TestApi, owner_url, week};
use uuid::Uuid;

const STOPPED: &str = "Mission arrêtée à la demande de son auteur.";

/// The kinds of the run's events, in order.
async fn kinds(api: &TestApi, run: Uuid) -> Vec<String> {
    let journal = events(api, tenant::ID, run).await;
    journal.into_iter().map(|(kind, _)| kind).collect()
}

/// The statuses of the run's requests.
async fn requests_of(run: Uuid) -> Vec<String> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let statuses = sqlx::query_scalar("select status from approvals where run_id = $1")
        .bind(run)
        .fetch_all(&owner)
        .await
        .unwrap();
    owner.close().await;
    statuses
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_queued_stage_stops_with_its_mission() {
    let api = TestApi::start().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    let id = entrust(&marc, "Mission arrêtée en file").await;
    assert_eq!(stop(&marc, id).await, StatusCode::OK);

    let mission = read(&marc, id).await;
    assert_eq!(mission["status"], "cancelled");
    assert_eq!(mission["error"], STOPPED);
    let run = stages(id).await[0].0;
    assert_eq!(stages(id).await[0].1, "cancelled");
    assert_eq!(kinds(&api, run).await, ["run_queued", "run_finished"]);
    let said = messages(&marc, &mission).await;
    assert_eq!(said.last().unwrap()["content"], STOPPED);
    // Never claimed, and not stopped twice.
    assert_eq!(take_queued(&api).await, 0);
    assert_eq!(stop(&marc, id).await, StatusCode::CONFLICT);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_stage_awaiting_a_decision_stops_with_its_requests() {
    let api = TestApi::start().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    let id = entrust(&marc, "Mission en attente").await;
    let mut search = week();
    search["status"] = json!(["in_progress"]);
    api.script(vec![
        ScriptedStep::calls("", vec![call("interventions_search", search)]),
        propose("INT-2049", "13:00"),
        apply_proposal("Je demande la validation."),
    ]);
    assert_eq!(take_queued(&api).await, 1);
    assert_eq!(read(&marc, id).await["status"], "awaiting_approval");
    let run = stages(id).await[0].0;
    assert_eq!(requests_of(run).await, ["pending"]);

    assert_eq!(stop(&marc, id).await, StatusCode::OK);
    assert_eq!(read(&marc, id).await["status"], "cancelled");
    assert_eq!(stages(id).await[0].1, "cancelled");
    assert_eq!(requests_of(run).await, ["cancelled"]);
}

// Several threads: the mission is stopped while the model step waits.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL"]
async fn a_running_stage_stopped_writes_and_calls_nothing_more() {
    let api = TestApi::start().await;
    executing(&api);
    let marc = api.sign_in(user::MARC).await;
    let (asked, waiting) = mpsc::channel();
    let (go_on, resume) = mpsc::channel::<()>();
    let resume = Mutex::new(resume);
    api.script(vec![ScriptedStep::dynamic(move |_| {
        asked.send(()).unwrap();
        tokio::task::block_in_place(|| resume.lock().unwrap().recv().unwrap());
        ScriptedReply::calls("", vec![call("interventions_search", week())])
    })]);
    let id = entrust(&marc, "Mission arrêtée en cours").await;
    let services = api.app.services.clone();
    let taking = tokio::spawn(async move {
        let runs = &services.runs;
        runs.take_queued(&services.shutdown).await.unwrap()
    });
    tokio::task::spawn_blocking(move || waiting.recv().unwrap())
        .await
        .unwrap();

    assert_eq!(stop(&marc, id).await, StatusCode::OK);
    go_on.send(()).unwrap();
    assert_eq!(taking.await.unwrap(), 1);
    api.idle().await;

    let run = stages(id).await[0].0;
    assert_eq!(stages(id).await[0].1, "cancelled");
    assert_eq!(
        kinds(&api, run).await,
        ["run_queued", "run_started", "run_finished"]
    );
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let called: i64 = sqlx::query_scalar("select count(*) from tool_invocations where run_id = $1")
        .bind(run)
        .fetch_one(&owner)
        .await
        .unwrap();
    owner.close().await;
    assert_eq!(called, 0);
    let mission = read(&marc, id).await;
    assert_eq!(mission["status"], "cancelled");
    let said = messages(&marc, &mission).await;
    assert_eq!(said.len(), 2);
    assert_eq!(said[1]["content"], STOPPED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mission_whose_stage_failed_elsewhere_is_not_stopped() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let id = entrust(&marc, "Mission balayée").await;
    // Its stage stuck running, then failed by the stale-run sweep.
    let run = stages(id).await[0].0;
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(
        "update agent_runs set status = 'running', updated_at = now() - interval '20 minutes' \
         where id = $1",
    )
    .bind(run)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
    let services = &api.app.services;
    let swept = sweep_stale_runs(&services.db, &services.bus, chrono::Duration::minutes(15))
        .await
        .unwrap();
    assert_eq!(swept, 1);
    let failed = read(&marc, id).await;
    assert_eq!(failed["status"], "failed");

    assert_eq!(stop(&marc, id).await, StatusCode::CONFLICT);
    let still = read(&marc, id).await;
    assert_eq!(still["status"], "failed");
    assert_eq!(still["error"], failed["error"]);
}

/// Waits until a query waits for a lock.
async fn someone_waits_for_a_lock(owner: &sqlx::PgPool) {
    for _ in 0..250 {
        let waiting: i64 = sqlx::query_scalar("select count(*) from pg_locks where not granted")
            .fetch_one(owner)
            .await
            .unwrap();
        if waiting > 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("nobody waits for a lock");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_stage_ending_while_its_mission_stops_has_its_next_one_cancelled() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let id = entrust(&marc, "Mission arrêtée entre deux étapes").await;
    let first = stages(id).await[0].0;
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    // The stage ends as an executor would: its run locked, then completed,
    // the next stage queued, in one transaction.
    let mut ending = owner.begin().await.unwrap();
    sqlx::query("select id from agent_runs where id = $1 for update")
        .bind(first)
        .execute(&mut *ending)
        .await
        .unwrap();
    let stopping = stop(&marc, id);
    let handing_over = async {
        someone_waits_for_a_lock(&owner).await;
        sqlx::query(
            "update agent_runs set status = 'completed', finished_at = now() where id = $1",
        )
        .bind(first)
        .execute(&mut *ending)
        .await
        .unwrap();
        let next: Uuid = sqlx::query_scalar(
            "insert into agent_runs (tenant_id, conversation_id, on_behalf_of_user_id, \
             objective, budget, status, mission_id) select tenant_id, conversation_id, \
             on_behalf_of_user_id, objective, budget, 'queued', mission_id from agent_runs \
             where id = $1 returning id",
        )
        .bind(first)
        .fetch_one(&mut *ending)
        .await
        .unwrap();
        sqlx::query("update missions set stages = 2 where id = $1")
            .bind(id)
            .execute(&mut *ending)
            .await
            .unwrap();
        ending.commit().await.unwrap();
        next
    };
    let (stopped, next) = tokio::join!(stopping, handing_over);
    owner.close().await;

    assert_eq!(stopped, StatusCode::OK);
    let statuses: Vec<(Uuid, String)> = stages(id).await;
    assert_eq!(
        statuses,
        [
            (first, "completed".to_owned()),
            (next, "cancelled".to_owned())
        ]
    );
    assert_eq!(read(&marc, id).await["status"], "cancelled");
}
