//! Retention applied (DATA_MODEL §5, ADR-185): the date of an account's
//! disabling and the sessions. The database dates an account's disabling
//! itself, whatever disables it, and forgets the date when it is enabled
//! again. A pass deletes the sessions ended thirty days ago and keeps the
//! more recent ones; the maintenance runs it from the start. A pass runs in
//! one process at a time. The memories: `retention_memories.rs`. The tests
//! that run a pass themselves start a process without maintenance.

mod support;

use std::time::Duration;

use mimorya_api::plugins::retention::{RETENTION_SWEEP, apply_retention};
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::TestApi;
use support::queue::API;
use support::retention::{account, disabled_at, owner, session, session_agents};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_database_dates_an_account_s_disabling() {
    let api = TestApi::start_with(API).await;
    let paul = api.sign_in(user::PAUL).await;
    let created = paul
        .post(
            "/admin/users",
            json!({ "email": "date.retention@neo-maintenance.example", "displayName": "Date" }),
        )
        .await;
    assert_eq!(created.status.as_u16(), 201, "{}", created.text());
    let id: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(disabled_at(id).await, None);

    // Disabled by the administration: dated; disabled again: the same date.
    let path = format!("/admin/users/{id}");
    let disabled = paul.patch(&path, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status.as_u16(), 200, "{}", disabled.text());
    let first = disabled_at(id).await.expect("dated");
    paul.patch(&path, json!({ "status": "disabled" })).await;
    assert_eq!(disabled_at(id).await, Some(first));

    // A statement cannot set it; enabled again, it is forgotten.
    let owner = owner().await;
    sqlx::query("update users set disabled_at = now() - interval '90 days' where id = $1")
        .bind(id)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    assert_eq!(disabled_at(id).await, Some(first));
    paul.patch(&path, json!({ "status": "active" })).await;
    assert_eq!(disabled_at(id).await, None);

    // Made disabled (by SCIM, by an operator): dated from its making.
    let made = account(
        tenant::ID,
        "inactif.retention@neo-maintenance.example",
        "disabled",
    )
    .await;
    assert!(disabled_at(made).await.is_some());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn ended_sessions_are_forgotten_thirty_days_on() {
    let api = TestApi::start_with(API).await;
    let paul = api.sign_in(user::PAUL).await;
    // Each session's revoking, idle term and absolute term, in days from now.
    for (agent, revoked, expires, absolute) in [
        ("retention-revoquee-31", Some(-31), 1, 30),
        ("retention-inactive-31", None, -31, 30),
        ("retention-absolue-31", None, 1, -31),
        ("retention-revoquee-29", Some(-29), 1, 30),
        ("retention-inactive-29", None, -29, 30),
    ] {
        session(tenant::ID, user::PAUL, agent, revoked, expires, absolute).await;
    }

    let retained = apply_retention(&api.app.services.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retained.sessions, 3);
    assert_eq!(
        session_agents("retention-").await,
        ["retention-inactive-29", "retention-revoquee-29"]
    );
    // The session in use is not touched.
    assert_eq!(paul.get("/auth/me").await.status.as_u16(), 200);

    // Another process running the pass: this one runs none.
    let db = &api.app.services.db;
    let held = db.try_sweep_lock(RETENTION_SWEEP).await.unwrap().unwrap();
    assert!(apply_retention(db).await.unwrap().is_none());
    held.release().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_maintenance_applies_retention_from_the_start() {
    // A session ended long before the process starts...
    session(tenant::ID, user::MARC, "demarrage-retention", None, -45, 30).await;
    let _api = TestApi::start().await;
    // ...is gone with the maintenance's first pass.
    for _ in 0..200 {
        if session_agents("demarrage-retention").await.is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the maintenance applied no retention at its start");
}
