//! Execution leases taken up (ADR-083): a run whose lease expired, or which
//! never had one, is taken up, in its own company, by a process that
//! executes runs, under its executor's lease; never one that executes
//! there, nor once renewed, nor once the service stops, and by no process
//! that does not execute runs. A live lease is left alone, at startup too
//! (ADR-086). Taken up, a run counts as having progressed: the sweep of
//! stuck runs leaves it to its next step. The service takes them up by
//! itself every so often.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use chrono::Duration;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_api::services::runs::{LeaseTiming, STOPPED_BEFORE_ITS_FIRST_STEP};
use mimorya_connectors::testing::serve_held_mcp_http;
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use support::TestApi;
use support::leases::{expire_running_lease_of, lease_of, lease_seen, lease_to};
use support::locks::{hold_rows, release_after};
use support::mcp::{PRIVATE_DEPLOYMENT, add, mcp_body};
use support::recovery::{events, interrupted_ago, restart, saved, status_and_error, stopped_run};
use support::runs::start_run;
use support::sweeps::sweep_stuck_from_elsewhere;
use uuid::Uuid;

async fn status(api: &TestApi, run: Uuid) -> String {
    status_and_error(api, tenant::ID, run).await.0
}

/// Takes up the runs whose lease expired, as the process that executes
/// runs; how many.
async fn take_up(api: &TestApi) -> usize {
    let runs = &api.app.services.runs;
    runs.start_executing().unwrap();
    runs.take_up_expired(&api.app.services.shutdown)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_whose_lease_expired_is_taken_up_in_its_company_under_this_executor_s() {
    let api = TestApi::start().await;
    let run = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    lease_to(run, Uuid::new_v4(), Duration::seconds(-1)).await;
    let foreign = stopped_run(&api, OTHER_TENANT_ID, OTHER_USER_ID, None, None, &[]).await;
    lease_to(foreign, Uuid::new_v4(), Duration::seconds(-2)).await;
    let seen = Arc::default();
    api.script(vec![lease_seen(&seen, user::MARC)]);

    let taken = take_up(&api).await;
    api.idle().await;
    assert_eq!(taken, 2);
    let executor = api.app.services.runs.executor_id();
    assert_eq!(
        seen.lock().unwrap().map(|lease| lease.0),
        Some(Some(executor))
    );
    assert_eq!(status(&api, run).await, "completed");
    assert_eq!(lease_of(run).await, (None, None));
    // Stopped before its first step, the other company's ends there, in
    // its own company.
    let (status, error) = status_and_error(&api, OTHER_TENANT_ID, foreign).await;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(STOPPED_BEFORE_ITS_FIRST_STEP));
    let ended = events(&api, OTHER_TENANT_ID, foreign).await;
    assert_eq!(ended.last().unwrap().0, "run_finished");
    assert!(events(&api, tenant::ID, foreign).await.is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_live_lease_is_left_alone_even_at_startup_and_none_is_taken_up() {
    let api = TestApi::start().await;
    let other = Uuid::new_v4();
    let held = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    lease_to(held, other, Duration::seconds(60)).await;
    let unleased = stopped_run(&api, tenant::ID, user::LUCAS, None, Some(saved()), &[]).await;

    // Never held by anyone: taken up; held by a live worker: left to it.
    api.script(vec![ScriptedStep::text("Repris.")]);
    assert_eq!(take_up(&api).await, 1);
    api.idle().await;
    assert_eq!(status(&api, unleased).await, "completed");
    assert_eq!(lease_of(held).await.0, Some(other));
    assert_eq!(status(&api, held).await, "running");
    // At startup too: another worker may hold it (ADR-086).
    assert_eq!(restart(&api).await, 0);
    assert_eq!(lease_of(held).await.0, Some(other));

    // Its worker gone, its lease expires: taken up.
    lease_to(held, other, Duration::seconds(-1)).await;
    api.script(vec![ScriptedStep::text("Repris.")]);
    assert_eq!(restart(&api).await, 1);
    assert_eq!(status(&api, held).await, "completed");
    assert_eq!(lease_of(held).await, (None, None));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_lease_renewed_while_it_is_taken_up_stays_its_executor_s() {
    let api = TestApi::start().await;
    let other = Uuid::new_v4();
    let run = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    lease_to(run, other, Duration::seconds(-1)).await;

    // Listed expired, then renewed by its executor before it is locked.
    let mut renewing = hold_rows(&api, "agent_runs", &[run]).await;
    sqlx::query(
        "update agent_runs set lease_expires_at = clock_timestamp() + interval '1 minute' \
         where id = $1",
    )
    .bind(run)
    .execute(&mut *renewing)
    .await
    .unwrap();
    let runs = &api.app.services.runs;
    runs.start_executing().unwrap();
    let (taken, ()) = tokio::join!(
        runs.take_up_expired(&api.app.services.shutdown),
        release_after(&api, 1, renewing)
    );
    assert_eq!(taken.unwrap(), 0);
    assert_eq!(lease_of(run).await.0, Some(other));
    assert_eq!(status(&api, run).await, "running");

    // Expired again, for good: taken up.
    api.script(vec![ScriptedStep::text("Repris.")]);
    lease_to(run, other, Duration::seconds(-1)).await;
    assert_eq!(take_up(&api).await, 1);
    api.idle().await;
    assert_eq!(status(&api, run).await, "completed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL"]
async fn a_run_executing_here_is_never_taken_up_from_under_it() {
    // Not renewed meanwhile: the lease expired below stays so.
    let api = TestApi::start_with_leases(LeaseTiming {
        renew_every: StdDuration::from_secs(3600),
        ..LeaseTiming::default()
    })
    .await;
    let marc = api.sign_in(user::MARC).await;
    let (expired, expired_seen) = std::sync::mpsc::channel::<()>();
    let (go, go_seen) = std::sync::mpsc::channel::<()>();
    let (expired, go_seen) = (Mutex::new(expired), Mutex::new(go_seen));
    api.script(vec![ScriptedStep::dynamic(move |_| {
        tokio::task::block_in_place(|| {
            expire_running_lease_of(user::MARC);
            expired.lock().unwrap().send(()).unwrap();
            go_seen.lock().unwrap().recv().unwrap();
        });
        ScriptedReply::text("Fait une seule fois.")
    })]);
    let run = start_run(&marc).await;
    tokio::task::spawn_blocking(move || expired_seen.recv().unwrap())
        .await
        .unwrap();

    let taken = take_up(&api).await;
    go.send(()).unwrap();
    api.idle().await;
    assert_eq!(taken, 0);
    assert_eq!(status(&api, run).await, "completed");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_service_takes_up_expired_leases_by_itself() {
    let api = TestApi::start_with_leases(LeaseTiming {
        take_up_every: StdDuration::from_millis(100),
        ..LeaseTiming::default()
    })
    .await;
    // Taken up at startup first: the takeover begins after.
    restart(&api).await;
    let run = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    api.script(vec![ScriptedStep::text("Repris.")]);
    lease_to(run, Uuid::new_v4(), Duration::seconds(-1)).await;

    let mut ended = String::new();
    for _ in 0..50 {
        api.idle().await;
        ended = status(&api, run).await;
        if ended != "running" {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(100)).await;
    }
    assert_eq!(ended, "completed");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_process_that_executes_runs_takes_leases_up() {
    let first = TestApi::start().await;
    let run = stopped_run(&first, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    lease_to(run, Uuid::new_v4(), Duration::seconds(-1)).await;

    // Not executing yet: neither its passes nor a call take anything up.
    let second = TestApi::start_with_leases(LeaseTiming {
        take_up_every: StdDuration::from_millis(100),
        ..LeaseTiming::default()
    })
    .await;
    let services = &second.app.services;
    let taken = services.runs.take_up_expired(&services.shutdown).await;
    assert_eq!(taken.unwrap(), 0);
    tokio::time::sleep(StdDuration::from_millis(300)).await;
    second.idle().await;
    assert_eq!(status(&first, run).await, "running");

    // Stopping: left to the next start.
    first.app.services.runs.start_executing().unwrap();
    let stopping = &first.app.services.shutdown;
    stopping.send_replace(true);
    let taken = take_up(&first).await;
    stopping.send_replace(false);
    assert_eq!(taken, 0);

    first.script(vec![ScriptedStep::text("Repris.")]);
    assert_eq!(take_up(&first).await, 1);
    first.idle().await;
    assert_eq!(status(&first, run).await, "completed");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_taken_up_is_not_taken_for_a_stuck_one_before_its_next_step() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let (url, hold) = serve_held_mcp_http().await;
    let connector = add(&paul, mcp_body("Procédures", "docs", &url, None)).await;
    let run = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    // Its last step twenty minutes ago, then its worker gone.
    interrupted_ago(run, 20).await;
    lease_to(run, Uuid::new_v4(), Duration::seconds(-1)).await;

    // Taken up, its connectors starting, it is swept by another process.
    api.script(vec![ScriptedStep::text("Repris.")]);
    assert_eq!(take_up(&api).await, 1);
    hold.reached().await;
    sweep_stuck_from_elsewhere();
    hold.release();
    api.idle().await;
    assert_eq!(status(&api, run).await, "completed");
    let deleted = paul.delete(&format!("/admin/connectors/{connector}")).await;
    assert!(deleted.status.is_success(), "{}", deleted.text());
}
