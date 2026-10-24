//! An execution that lost its run's lease (ADR-086): once another executor
//! took the lease up, the first one writes nothing more for the run (no
//! event, no tool called, no outcome) and records no crash: the run is the
//! other's. A renewal that no longer finds the lease stops the execution
//! before its next step, even should the lease come back meanwhile.

mod support;

use std::sync::{Mutex, mpsc};

use chrono::Duration;

use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call};
use mimorya_core::demo::{tenant, user};
use support::leases::{hand_running_lease_to, lease_of, lease_to};
use support::recovery::{events, status_and_error};
use support::runs::start_run;
use support::{TestApi, owner_url, week};
use uuid::Uuid;

/// The kinds of the run's events, in order.
async fn kinds(api: &TestApi, run: Uuid) -> Vec<String> {
    let journal = events(api, tenant::ID, run).await;
    journal.into_iter().map(|(kind, _)| kind).collect()
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
    back.send(()).unwrap();
    api.idle().await;

    // Stopped before concluding: no answer, nothing ended.
    assert_eq!(kinds(&api, run).await, ["run_started"]);
    assert_eq!(status_and_error(&api, tenant::ID, run).await.0, "running");
    end_elsewhere(run).await;
}
