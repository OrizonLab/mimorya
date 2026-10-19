//! Run state transitions: a finished run is never rewritten; only a final
//! `run_finished` counts as the run's announced end.

mod support;

use mimorya_core::autonomy::AutonomyLevel;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::run_events::{append_run_event, run_end_recorded};
use mimorya_db::repositories::runs::{
    NewRun, RunPatch, RunStatus, create_run, get_run, update_active_run, update_run,
};
use serde_json::json;

fn new_run() -> NewRun {
    NewRun {
        tenant_id: tenant::ID,
        conversation_id: None,
        initiated_by_user_id: user::MARC,
        on_behalf_of_user_id: user::MARC,
        objective: "Test".into(),
        success_criteria: Vec::new(),
        autonomy_level: AutonomyLevel::ExecuteWithApproval,
        budget: json!({}),
    }
}

fn status(status: RunStatus) -> RunPatch {
    RunPatch {
        status: Some(status),
        ..RunPatch::default()
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn never_rewrites_a_finished_run() {
    let urls = support::urls(|_| {});
    let db = support::connect(&urls.app).await;
    // Rolled back when dropped: nothing is left behind.
    let mut tx = db.begin(tenant::ID).await.unwrap();

    // Finished (failed by the stale-run sweeper, for one), then a late
    // outcome arrives.
    for finished in RunStatus::FINISHED {
        let run = create_run(&mut tx, new_run()).await.unwrap();
        update_run(&mut tx, run.id, status(finished)).await.unwrap();
        let late = if finished == RunStatus::Failed {
            RunStatus::Completed
        } else {
            RunStatus::Failed
        };
        let rewritten = update_active_run(&mut tx, run.id, status(late))
            .await
            .unwrap();
        assert!(!rewritten, "{finished:?}");
        let stored = get_run(&mut tx, run.id).await.unwrap().unwrap();
        assert_eq!(stored.status(), finished);
    }

    // Every other status is still updated.
    for current in RunStatus::ALL.into_iter().filter(|s| !s.is_finished()) {
        let run = create_run(&mut tx, new_run()).await.unwrap();
        update_run(&mut tx, run.id, status(current)).await.unwrap();
        let updated = update_active_run(&mut tx, run.id, status(RunStatus::Completed))
            .await
            .unwrap();
        assert!(updated, "{current:?}");
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_final_status_counts_as_the_run_s_announced_end() {
    let urls = support::urls(|_| {});
    let db = support::connect(&urls.app).await;
    // Rolled back when dropped: nothing is left behind.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let run = create_run(&mut tx, new_run()).await.unwrap();
    assert!(!run_end_recorded(&mut tx, run.id).await.unwrap());

    // A pause for validation announces no end.
    let paused = json!({ "status": "awaiting_approval" });
    append_run_event(&mut tx, tenant::ID, run.id, "run_finished", &paused)
        .await
        .unwrap();
    assert!(!run_end_recorded(&mut tx, run.id).await.unwrap());

    let failed = json!({ "status": "failed", "error": "Interrompu" });
    append_run_event(&mut tx, tenant::ID, run.id, "run_finished", &failed)
        .await
        .unwrap();
    assert!(run_end_recorded(&mut tx, run.id).await.unwrap());
}
