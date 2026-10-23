//! Execution leases (ADR-083): a run executes under the lease of the
//! executor that set it running (its turn, a decision, an expiry), renewed
//! while it executes and given up once it pauses or ends. A run whose lease
//! expired is taken up by an executor that does not execute it, never from
//! under its own nor once renewed, by itself every so often. A live lease,
//! or none, is left alone, but not at startup, when this process alone
//! executes (ADR-053).

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_api::services::runs::LeaseTiming;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::TestApi;
use support::leases::{Lease, expire_running_lease_of, lease_of, lease_to, running_lease_of};
use support::locks::{hold_rows, release_after};
use support::manager_approvals::{applying_then, due_in};
use support::recovery::{restart, saved, status_and_error, stopped_run};
use support::runs::start_run;
use uuid::Uuid;

/// Brief leases, renewed often: what a run's step sees change in a second.
const BRIEF: LeaseTiming = LeaseTiming {
    ttl: StdDuration::from_secs(2),
    renew_every: StdDuration::from_millis(200),
    take_up_every: StdDuration::from_secs(60),
};

async fn status(api: &TestApi, run: Uuid) -> String {
    status_and_error(api, tenant::ID, run).await.0
}

/// A model's step that records the lease of the run `user_id`'s agent is
/// running, as it goes on.
fn lease_seen(seen: &Arc<Mutex<Option<Lease>>>, user_id: Uuid) -> ScriptedStep {
    let seen = seen.clone();
    ScriptedStep::dynamic(move |_| {
        *seen.lock().unwrap() = Some(running_lease_of(user_id));
        ScriptedReply::text("Repris.")
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL"]
async fn a_run_executes_under_its_lease_renewed_until_it_ends() {
    let api = TestApi::start_with_leases(BRIEF).await;
    let marc = api.sign_in(user::MARC).await;
    let seen: Arc<Mutex<Vec<Lease>>> = Arc::default();
    let seen_by_model = seen.clone();
    api.script(vec![ScriptedStep::dynamic(move |_| {
        // Waiting without holding the runtime's other tasks back.
        tokio::task::block_in_place(|| {
            let first = running_lease_of(user::MARC);
            std::thread::sleep(StdDuration::from_millis(700));
            let later = running_lease_of(user::MARC);
            seen_by_model.lock().unwrap().extend([first, later]);
        });
        ScriptedReply::text("Point fait.")
    })]);
    let run = start_run(&marc).await;
    api.idle().await;

    let seen = seen.lock().unwrap().clone();
    let executor = api.app.services.runs.executor_id();
    assert_eq!(seen[0].0, Some(executor), "{seen:?}");
    assert!(seen[1].1 > seen[0].1, "not renewed: {seen:?}");
    // Ended: nobody's.
    assert_eq!(status(&api, run).await, "completed");
    assert_eq!(lease_of(run).await, (None, None));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_paused_run_belongs_to_nobody_until_set_going_again_here() {
    let api = TestApi::start().await;
    let nadia = api.sign_in(user::NADIA).await;
    let executor = api.app.services.runs.executor_id();

    // Decided.
    let seen = Arc::default();
    let slot = ("INT-2049", "09:00");
    let (run, approval) = applying_then(&api, &nadia, slot, lease_seen(&seen, user::NADIA)).await;
    let run: Uuid = run.parse().unwrap();
    assert_eq!(status(&api, run).await, "awaiting_approval");
    assert_eq!(lease_of(run).await, (None, None));
    let approval = approval.expect("a request she decides");
    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    nadia.post(&decision, json!({ "approved": false })).await;
    api.idle().await;
    assert_eq!(
        seen.lock().unwrap().map(|lease| lease.0),
        Some(Some(executor))
    );
    assert_eq!(status(&api, run).await, "completed");
    assert_eq!(lease_of(run).await, (None, None));

    // Expired.
    let seen = Arc::default();
    let slot = ("INT-2049", "10:00");
    let (run, approval) = applying_then(&api, &nadia, slot, lease_seen(&seen, user::NADIA)).await;
    let approval: Uuid = approval.expect("a request")["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    due_in(&api, tenant::ID, approval, -1).await;
    // Expired by the process that executes runs (ADR-061).
    api.app.services.runs.claim_execution().await.unwrap();
    assert_eq!(api.app.services.runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;
    assert_eq!(
        seen.lock().unwrap().map(|lease| lease.0),
        Some(Some(executor))
    );
    assert_eq!(status(&api, run.parse().unwrap()).await, "completed");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_whose_lease_expired_is_taken_up_under_this_executor_s() {
    let api = TestApi::start().await;
    let run = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    lease_to(run, Uuid::new_v4(), Duration::seconds(-1)).await;
    let seen = Arc::default();
    api.script(vec![lease_seen(&seen, user::MARC)]);

    let taken = api.app.services.runs.take_up_expired(Utc::now()).await;
    api.idle().await;
    assert_eq!(taken.unwrap(), 1);
    let executor = api.app.services.runs.executor_id();
    assert_eq!(
        seen.lock().unwrap().map(|lease| lease.0),
        Some(Some(executor))
    );
    assert_eq!(status(&api, run).await, "completed");
    assert_eq!(lease_of(run).await, (None, None));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_live_lease_or_none_is_left_alone_until_startup() {
    let api = TestApi::start().await;
    let other = Uuid::new_v4();
    let held = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    lease_to(held, other, Duration::seconds(60)).await;
    let unleased = stopped_run(&api, tenant::ID, user::LUCAS, None, Some(saved()), &[]).await;

    let taken = api.app.services.runs.take_up_expired(Utc::now()).await;
    assert_eq!(taken.unwrap(), 0);
    assert_eq!(lease_of(held).await.0, Some(other));
    assert_eq!(lease_of(unleased).await, (None, None));
    assert_eq!(status(&api, held).await, "running");

    // At startup, this process alone executes (ADR-053): both taken up.
    api.script(vec![
        ScriptedStep::text("Repris."),
        ScriptedStep::text("Repris."),
    ]);
    assert_eq!(restart(&api).await, 2);
    for run in [held, unleased] {
        assert_eq!(status(&api, run).await, "completed");
        assert_eq!(lease_of(run).await, (None, None));
    }
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
        "update agent_runs set lease_expires_at = now() + interval '1 minute' where id = $1",
    )
    .bind(run)
    .execute(&mut *renewing)
    .await
    .unwrap();
    let runs = &api.app.services.runs;
    let (taken, ()) = tokio::join!(
        runs.take_up_expired(Utc::now()),
        release_after(&api, 1, renewing)
    );
    assert_eq!(taken.unwrap(), 0);
    assert_eq!(lease_of(run).await.0, Some(other));
    assert_eq!(status(&api, run).await, "running");

    // Expired again, for good: taken up.
    api.script(vec![ScriptedStep::text("Repris.")]);
    lease_to(run, other, Duration::seconds(-1)).await;
    assert_eq!(runs.take_up_expired(Utc::now()).await.unwrap(), 1);
    api.idle().await;
    assert_eq!(status(&api, run).await, "completed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL"]
async fn a_run_executing_here_is_never_taken_up_from_under_it() {
    // Leases renewed every 20 s: the one expired below stays so.
    let api = TestApi::start().await;
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

    let taken = api.app.services.runs.take_up_expired(Utc::now()).await;
    go.send(()).unwrap();
    api.idle().await;
    assert_eq!(taken.unwrap(), 0);
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
