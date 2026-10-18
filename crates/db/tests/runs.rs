//! Run state transitions: a finished run is never rewritten.

mod support;

use mimorya_core::autonomy::AutonomyLevel;
use mimorya_core::demo::{tenant, user};
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

    // Failed by the stale-run sweeper, then a late outcome arrives.
    let swept = create_run(&mut tx, new_run()).await.unwrap();
    update_run(&mut tx, swept.id, status(RunStatus::Failed))
        .await
        .unwrap();
    let rewritten = update_active_run(&mut tx, swept.id, status(RunStatus::Completed))
        .await
        .unwrap();
    assert!(!rewritten);
    let stored = get_run(&mut tx, swept.id).await.unwrap().unwrap();
    assert_eq!(stored.status(), RunStatus::Failed);

    let active = create_run(&mut tx, new_run()).await.unwrap();
    let updated = update_active_run(&mut tx, active.id, status(RunStatus::Completed))
        .await
        .unwrap();
    assert!(updated);
}
