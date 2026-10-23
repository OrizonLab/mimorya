//! Execution leases held (ADR-083): a run executes under the lease of the
//! executor that set it running (its turn, a decision, an expiry), renewed
//! while it executes and given up once it pauses or ends. Taken up once
//! expired: `run_lease_takeover.rs`.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_api::services::runs::LeaseTiming;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::TestApi;
use support::leases::{Lease, lease_of, lease_seen, running_lease_of};
use support::manager_approvals::{applying_then, due_in};
use support::recovery::status_and_error;
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
