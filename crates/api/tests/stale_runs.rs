//! Runs left `running` by a crashed or stuck process are failed by the
//! periodic sweep, and the clients following them learn it at once: the
//! `run_finished` event is recorded (replayed by the event stream) and
//! broadcast. A run still making progress is left alone.

mod support;

use std::time::Duration;

use mimorya_api::plugins::maintenance::sweep_stale_runs;
use mimorya_core::autonomy::AutonomyLevel;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::run_events::list_run_events;
use mimorya_db::repositories::runs::{NewRun, RunStatus, create_run, get_run};
use serde_json::json;
use support::{TestApi, owner_url};
use uuid::Uuid;

const INTERRUPTED: &str = "Exécution interrompue (service redémarré ou bloqué)";

async fn running_run(api: &TestApi) -> Uuid {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let run = create_run(
        &mut tx,
        NewRun {
            tenant_id: tenant::ID,
            conversation_id: None,
            initiated_by_user_id: user::MARC,
            on_behalf_of_user_id: user::MARC,
            objective: "Synthèse".into(),
            success_criteria: Vec::new(),
            autonomy_level: AutonomyLevel::ExecuteWithApproval,
            budget: json!({}),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    run.id
}

/// As if the run had made no progress for twenty minutes.
async fn age(run_id: Uuid) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("update agent_runs set updated_at = now() - interval '20 minutes' where id = $1")
        .bind(run_id)
        .execute(&owner)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_swept_run_announces_its_end_to_its_followers() {
    let api = TestApi::start().await;
    let services = &api.app.services;
    let stuck = running_run(&api).await;
    let alive = running_run(&api).await;
    let mut followers = services.bus.subscribe(stuck);
    age(stuck).await;

    // The app's own sweep may run meanwhile: either way, the end is announced.
    sweep_stale_runs(&services.db, &services.bus, chrono::Duration::minutes(15))
        .await
        .unwrap();

    let event = tokio::time::timeout(Duration::from_secs(5), followers.recv())
        .await
        .expect("run_finished broadcast in time")
        .expect("subscribed");
    assert_eq!(event.event_type, "run_finished");
    assert_eq!(
        event.data,
        json!({ "status": "failed", "error": INTERRUPTED })
    );
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    let recorded = list_run_events(&mut tx, stuck, 0).await.unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].event_type, "run_finished");
    assert_eq!(Some(recorded[0].seq), event.seq);
    let stuck_run = get_run(&mut tx, stuck).await.unwrap().unwrap();
    assert_eq!(stuck_run.status(), RunStatus::Failed);
    assert_eq!(stuck_run.error.as_deref(), Some(INTERRUPTED));
    let alive_run = get_run(&mut tx, alive).await.unwrap().unwrap();
    assert_eq!(alive_run.status(), RunStatus::Running);
    assert!(list_run_events(&mut tx, alive, 0).await.unwrap().is_empty());
}
