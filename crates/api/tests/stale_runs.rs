//! Runs left `running` by a crashed or stuck process are failed by the
//! periodic sweep, and the clients following them learn it at once: the
//! `run_finished` event is recorded (replayed by the event stream) in each
//! run's own company, and broadcast. A run still making progress is left
//! alone.

mod support;

use std::time::Duration;

use mimorya_api::plugins::maintenance::sweep_stale_runs;
use mimorya_core::autonomy::AutonomyLevel;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::run_events::{list_run_events, run_end_recorded};
use mimorya_db::repositories::runs::{NewRun, RunStatus, create_run, get_run};
use serde_json::json;
use support::companies::create;
use support::{TestApi, owner_url};
use uuid::Uuid;

const INTERRUPTED: &str = "Exécution interrompue (service redémarré ou bloqué)";

async fn running_run(api: &TestApi, tenant_id: Uuid, user_id: Uuid) -> Uuid {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let run = create_run(
        &mut tx,
        NewRun {
            tenant_id,
            conversation_id: None,
            initiated_by_user_id: user_id,
            on_behalf_of_user_id: user_id,
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
async fn a_swept_run_announces_its_end_to_its_followers_in_its_own_company() {
    let api = TestApi::start().await;
    let services = &api.app.services;
    let other = create(&api, "balayage", "admin@balayage.example").await;
    let stuck = running_run(&api, tenant::ID, user::MARC).await;
    let stuck_elsewhere = running_run(&api, other.tenant_id, other.admin_id).await;
    let alive = running_run(&api, tenant::ID, user::MARC).await;
    let mut followers = services.bus.subscribe(stuck);
    let mut followers_elsewhere = services.bus.subscribe(stuck_elsewhere);
    age(stuck).await;
    age(stuck_elsewhere).await;

    // The app's own sweep may run meanwhile: either way, the end is announced.
    sweep_stale_runs(&services.db, &services.bus, chrono::Duration::minutes(15))
        .await
        .unwrap();

    for followers in [&mut followers, &mut followers_elsewhere] {
        let event = tokio::time::timeout(Duration::from_secs(5), followers.recv())
            .await
            .expect("run_finished broadcast in time")
            .expect("subscribed");
        assert_eq!(event.event_type, "run_finished");
        assert_eq!(
            event.data,
            json!({ "status": "failed", "error": INTERRUPTED })
        );
    }
    for (company, run, foreign) in [
        (tenant::ID, stuck, stuck_elsewhere),
        (other.tenant_id, stuck_elsewhere, stuck),
    ] {
        let mut tx = services.db.begin(company).await.unwrap();
        let recorded = list_run_events(&mut tx, run, 0).await.unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].event_type, "run_finished");
        assert!(run_end_recorded(&mut tx, run).await.unwrap());
        let swept = get_run(&mut tx, run).await.unwrap().unwrap();
        assert_eq!(swept.status(), RunStatus::Failed);
        assert_eq!(swept.error.as_deref(), Some(INTERRUPTED));
        // Each company sees its own run's journal only.
        assert!(
            list_run_events(&mut tx, foreign, 0)
                .await
                .unwrap()
                .is_empty()
        );
    }
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    let alive_run = get_run(&mut tx, alive).await.unwrap().unwrap();
    assert_eq!(alive_run.status(), RunStatus::Running);
    assert!(!run_end_recorded(&mut tx, alive).await.unwrap());
}
