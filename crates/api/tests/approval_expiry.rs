//! Requests nobody decided in time (ADR-061): the process that executes runs
//! expires them, audited as the platform's, and lets their run go on once
//! nothing else waits, with the rights of whom the agent acts for, or ends
//! it if that account is no longer active.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_agent::runtime::ACCOUNT_INACTIVE;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::approvals::{ApprovalRouting, NewApproval, create_approval};
use serde_json::{Value, json};
use support::manager_approvals::{applying, listed};
use support::{Client, TestApi};
use uuid::Uuid;

fn id_of(approval: &Value) -> Uuid {
    approval["id"].as_str().unwrap().parse().unwrap()
}

/// Moves the request's deadline a minute into the past.
async fn overdue(api: &TestApi, approval: Uuid) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update approvals set expires_at = now() - interval '1 minute' where id = $1")
        .bind(approval)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// The status of the request `id` in `client`'s approvals list.
async fn status_of(client: &Client<'_>, id: Uuid) -> Value {
    client.get("/approvals?status=all").await.json()["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == json!(id))
        .map(|a| a["status"].clone())
        .unwrap_or_default()
}

async fn run_status(client: &Client<'_>, run_id: &str) -> Value {
    client.get(&format!("/runs/{run_id}")).await.json()["run"].clone()
}

/// The platform's expiries audited for the run: reason and metadata.
async fn expiries(api: &TestApi, run_id: &str) -> Vec<(Option<String>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<(Option<String>, Value)> = sqlx::query_as(
        "select reason, metadata from audit_log where run_id = $1::uuid \
         and action = 'approval.expired' and actor_type = 'system' \
         and actor_user_id is null and on_behalf_of_user_id = $2 \
         and outcome = 'failure' order by id",
    )
    .bind(run_id)
    .bind(user::NADIA)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_overdue_request_expires_and_its_run_goes_on_without_the_action() {
    let api = TestApi::start().await;
    let nadia = api.sign_in(user::NADIA).await;
    let (run_id, approval) = applying(
        &api,
        &nadia,
        ("INT-2049", "09:00"),
        "Le planning n'a pas été appliqué.",
    )
    .await;
    let approval = approval.expect("a request she decides");
    let runs = &api.app.services.runs;

    // Not yet due: nothing expires.
    runs.claim_execution().await.unwrap();
    assert_eq!(runs.expire_overdue().await.unwrap(), 0);
    overdue(&api, id_of(&approval)).await;
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;

    let expired = listed(&nadia, &run_id).await.unwrap();
    assert_eq!(expired["status"], "expired");
    assert_eq!(expired["decidedBy"], Value::Null);
    let run = run_status(&nadia, &run_id).await;
    assert_eq!(run["status"], "completed", "{run}");
    assert_eq!(
        expiries(&api, &run_id).await,
        [(
            Some("Délai de validation dépassé".to_string()),
            json!({ "toolName": "planning_apply", "risk": "write", "routing": "requester" })
        )]
    );
    // Once only.
    assert_eq!(runs.expire_overdue().await.unwrap(), 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_process_that_executes_runs_expires_requests() {
    let api = TestApi::start().await;
    let nadia = api.sign_in(user::NADIA).await;
    let (run_id, approval) = applying(&api, &nadia, ("INT-2052", "09:00"), "Inchangé.").await;
    let approval = approval.expect("a request she decides");
    overdue(&api, id_of(&approval)).await;

    // Without the run executor's lock, it leaves them to whoever holds it.
    let runs = &api.app.services.runs;
    assert_eq!(runs.expire_overdue().await.unwrap(), 0);
    assert_eq!(listed(&nadia, &run_id).await.unwrap()["status"], "pending");
    runs.claim_execution().await.unwrap();
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;
    assert_eq!(listed(&nadia, &run_id).await.unwrap()["status"], "expired");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_waits_while_another_of_its_requests_may_still_be_decided() {
    let api = TestApi::start().await;
    let nadia = api.sign_in(user::NADIA).await;
    let (run_id, approval) = applying(&api, &nadia, ("INT-2049", "10:00"), "Inchangé.").await;
    let approval = approval.expect("a request she decides");
    // A second request of the run, due tomorrow.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let other = create_approval(
        &mut tx,
        NewApproval {
            tenant_id: tenant::ID,
            run_id: run_id.parse().unwrap(),
            requested_for_user_id: user::NADIA,
            tool_name: "planning_apply".into(),
            tool_call_id: "call-other".into(),
            action_summary: "Appliquer un autre planning".into(),
            payload: json!({}),
            risk: "write".into(),
            reason: "Action modifiant des données".into(),
            expires_at: Some(Utc::now() + Duration::days(1)),
            routing: ApprovalRouting::Requester,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let runs = &api.app.services.runs;
    runs.claim_execution().await.unwrap();
    overdue(&api, id_of(&approval)).await;
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;
    assert_eq!(status_of(&nadia, id_of(&approval)).await, "expired");
    assert_eq!(status_of(&nadia, other.id).await, "pending");
    let run = run_status(&nadia, &run_id).await;
    assert_eq!(run["status"], "awaiting_approval", "{run}");

    // The other one due in turn, the run goes on.
    overdue(&api, other.id).await;
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;
    assert_eq!(status_of(&nadia, other.id).await, "expired");
    let run = run_status(&nadia, &run_id).await;
    assert_eq!(run["status"], "completed", "{run}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_run_of_an_account_no_longer_active_ends_when_its_request_expires() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let (run_id, approval) = applying(&api, &nadia, ("INT-2052", "10:00"), "Inchangé.").await;
    let approval = approval.expect("a request she decides");
    let account = format!("/admin/users/{}", user::NADIA);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());

    let runs = &api.app.services.runs;
    runs.claim_execution().await.unwrap();
    overdue(&api, id_of(&approval)).await;
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());

    let nadia = api.sign_in(user::NADIA).await;
    let run = run_status(&nadia, &run_id).await;
    assert_eq!(run["status"], "failed", "{run}");
    assert_eq!(run["error"], ACCOUNT_INACTIVE);
    assert_eq!(listed(&nadia, &run_id).await.unwrap()["status"], "expired");
}
