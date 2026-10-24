//! An execution that lost its run's lease (ADR-086): once another executor
//! took the lease up, the first one writes nothing more for the run (no
//! event, no tool called, no outcome) and records no crash, even crashing,
//! whether the run still runs or was paused since: the run is the other's.
//! A call that returns once the lease went elsewhere is still recorded and
//! audited: it happened. A renewal that no longer finds the lease stops the
//! execution before its next step, its answer not even streamed, even
//! should the lease come back meanwhile.

mod support;

use std::sync::{Mutex, mpsc};

use chrono::Duration;

use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call};
use mimorya_connectors::testing::serve_held_mcp_http;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::leases::{hand_running_lease_to, lease_of, lease_to, pause_running_run_of};
use support::mcp::{PRIVATE_DEPLOYMENT, add, mcp_body};
use support::recovery::{events, status_and_error};
use support::relay::next_event;
use support::runs::start_run;
use support::{TestApi, owner_url, week};
use uuid::Uuid;

/// The kinds of the run's events, in order.
async fn kinds(api: &TestApi, run: Uuid) -> Vec<String> {
    let journal = events(api, tenant::ID, run).await;
    journal.into_iter().map(|(kind, _)| kind).collect()
}

/// What the run left: its calls recorded, its calls audited, and its
/// answers in the conversation.
async fn left(api: &TestApi, run: Uuid) -> (i64, i64, i64) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let counts = sqlx::query_as(
        "select (select count(*) from tool_invocations where run_id = $1), \
         (select count(*) from audit_log where run_id = $1 and action = 'agent.tool.execute'), \
         (select count(*) from messages where run_id = $1 and role = 'assistant')",
    )
    .bind(run)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    drop(tx);
    counts
}

/// Ends the run left to the other executor, as it would.
async fn end_elsewhere(run: Uuid) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("update agent_runs set status = 'failed' where id = $1")
        .bind(run)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_execution_whose_lease_went_elsewhere_writes_and_calls_nothing_more() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let other = Uuid::new_v4();
    api.script(vec![ScriptedStep::dynamic(move |_| {
        // Taken up meanwhile by another executor.
        hand_running_lease_to(user::MARC, other);
        ScriptedReply::calls("", vec![call("interventions_search", week())])
    })]);
    let run = start_run(&marc).await;
    api.idle().await;

    // Neither the call nor anything after it: the run is the other's.
    assert_eq!(kinds(&api, run).await, ["run_started"]);
    assert_eq!(status_and_error(&api, tenant::ID, run).await.0, "running");
    assert_eq!(lease_of(run).await.0, Some(other));
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let called: i64 = sqlx::query_scalar("select count(*) from tool_invocations where run_id = $1")
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    drop(tx);
    assert_eq!(called, 0);
    assert_eq!(api.model.as_ref().unwrap().requests().len(), 1);
    end_elsewhere(run).await;
}

// Several threads: the test renews while the model step waits.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL"]
async fn a_renewal_that_no_longer_finds_the_lease_stops_the_execution() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let (handed, gone) = mpsc::channel();
    let (back, resume) = mpsc::channel::<()>();
    let resume = Mutex::new(resume);
    api.script(vec![ScriptedStep::dynamic(move |_| {
        // Gone while a renewal looks for it, then back before the answer.
        hand_running_lease_to(user::MARC, Uuid::new_v4());
        handed.send(()).unwrap();
        tokio::task::block_in_place(|| resume.lock().unwrap().recv().unwrap());
        ScriptedReply::text("Semaine calme.")
    })]);
    let run = start_run(&marc).await;
    let runs = &api.app.services.runs;
    tokio::task::spawn_blocking(move || gone.recv().unwrap())
        .await
        .unwrap();
    assert_eq!(runs.renew_leases().await, 0);
    lease_to(run, runs.executor_id(), Duration::minutes(1)).await;
    let mut followed = api.app.services.bus.subscribe(tenant::ID, run);
    back.send(()).unwrap();
    api.idle().await;

    // Stopped before concluding: no answer, not even streamed, nothing
    // ended.
    assert!(next_event(&mut followed, 0.5).await.is_none());
    assert_eq!(kinds(&api, run).await, ["run_started"]);
    assert_eq!(status_and_error(&api, tenant::ID, run).await.0, "running");
    end_elsewhere(run).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_execution_that_crashes_once_its_lease_went_elsewhere_records_nothing() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let other = Uuid::new_v4();
    api.script(vec![ScriptedStep::dynamic(move |_| {
        hand_running_lease_to(user::MARC, other);
        panic!("crashed once the lease went elsewhere");
    })]);
    let run = start_run(&marc).await;
    api.idle().await;

    // Neither failed nor ended: the run is the other's.
    assert_eq!(kinds(&api, run).await, ["run_started"]);
    assert_eq!(
        status_and_error(&api, tenant::ID, run).await,
        ("running".into(), None)
    );
    assert_eq!(lease_of(run).await.0, Some(other));
    assert_eq!(left(&api, run).await, (0, 0, 0));
    end_elsewhere(run).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_execution_that_crashes_once_its_run_was_paused_elsewhere_leaves_the_pause() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    api.script(vec![ScriptedStep::dynamic(|_| {
        // Taken up, then paused, by another executor.
        pause_running_run_of(user::MARC);
        panic!("crashed once the run was paused elsewhere");
    })]);
    let run = start_run(&marc).await;
    api.idle().await;

    assert_eq!(kinds(&api, run).await, ["run_started"]);
    let paused = status_and_error(&api, tenant::ID, run).await;
    assert_eq!(paused, ("awaiting_approval".into(), None));
    assert_eq!(left(&api, run).await, (0, 0, 0));
    end_elsewhere(run).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_call_that_returns_once_the_lease_went_elsewhere_is_recorded_and_audited() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let (url, hold) = serve_held_mcp_http().await;
    hold.release();
    let connector = add(&paul, mcp_body("Procédures", "docs", &url, None)).await;
    let tool = "mcp_docs_search_procedures";
    let policy = json!({ tool: { "risk": "read", "permissions": ["intervention:read"] } });
    let path = format!("/admin/connectors/{connector}");
    let mapped = paul.patch(&path, json!({ "toolPolicies": policy })).await;
    assert!(mapped.status.is_success(), "{}", mapped.text());
    let held = hold.clone();
    api.script(vec![ScriptedStep::dynamic(move |_| {
        // The call held by the server, once called.
        held.hold_again();
        ScriptedReply::calls("", vec![call(tool, json!({ "query": "circulateur" }))])
    })]);
    let marc = api.sign_in(user::MARC).await;
    let run = start_run(&marc).await;

    // Taken up by another executor while the server works.
    hold.holding().await;
    let other = Uuid::new_v4();
    lease_to(run, other, Duration::minutes(1)).await;
    hold.release();
    api.idle().await;
    let lease = lease_of(run).await.0;
    end_elsewhere(run).await;
    let deleted = paul.delete(&path).await;
    assert!(deleted.status.is_success(), "{}", deleted.text());

    // Called and audited; its result, and all that follows, the other's.
    let called = ["run_started", "usage", "tool_call"];
    assert_eq!(kinds(&api, run).await, called);
    assert_eq!(left(&api, run).await, (1, 1, 0));
    assert_eq!(lease, Some(other));
}
