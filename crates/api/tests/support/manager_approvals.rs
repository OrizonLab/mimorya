//! Validation by a manager (ADR-059): the routing setting, a scripted run
//! asking to apply a planning, a request of the other company, and what the
//! lists and the audit say of them.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::tenant;
use mimorya_db::repositories::approvals::{ApprovalRouting, NewApproval, create_approval};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use uuid::Uuid;

use super::planning::{apply_proposal, propose};
use super::recovery::{saved, stopped_run};
use super::{Client, TestApi};

pub const ROUTING: &str = "/admin/approval-routing";

/// Has the company's managers validate the actions of `risks`.
pub async fn route_to_managers(paul: &Client<'_>, risks: &[&str]) {
    let set = paul.put(ROUTING, json!({ "managerRisks": risks })).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
}

/// Has `requester` propose then apply a slot for `intervention`, and returns
/// the run and what the requester's approvals list says of its request.
pub async fn applying(
    api: &TestApi,
    requester: &Client<'_>,
    slot: (&str, &str),
    final_text: &str,
) -> (String, Option<Value>) {
    applying_then(api, requester, slot, ScriptedStep::text(final_text)).await
}

/// As `applying`, the run going on with `last` once its request ends.
pub async fn applying_then(
    api: &TestApi,
    requester: &Client<'_>,
    (intervention, start_time): (&str, &str),
    last: ScriptedStep,
) -> (String, Option<Value>) {
    api.script(vec![
        propose(intervention, start_time),
        apply_proposal(""),
        last,
    ]);
    let conversation = requester.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let started = requester
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": format!("Planifie {intervention}") }),
        )
        .await
        .json();
    let run_id = started["runId"].as_str().unwrap().to_string();
    api.idle().await;
    let approval = listed(requester, &run_id).await;
    (run_id, approval)
}

/// The run's request in `client`'s approvals list.
pub async fn listed(client: &Client<'_>, run_id: &str) -> Option<Value> {
    client.get("/approvals?status=all").await.json()["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["runId"] == run_id)
        .cloned()
}

/// Moves the request's deadline to `minutes` from now.
pub async fn due_in(api: &TestApi, tenant_id: Uuid, approval: Uuid, minutes: i64) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query("update approvals set expires_at = $2 where id = $1")
        .bind(approval)
        .bind(Utc::now() + Duration::minutes(minutes))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// Moves back when the request was made by `minutes`: past ten, its
/// notice, never sent, is taken for lost (ADR-086).
pub async fn made_ago(api: &TestApi, tenant_id: Uuid, approval: Uuid, minutes: i64) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query("update approvals set created_at = $2 where id = $1")
        .bind(approval)
        .bind(Utc::now() - Duration::minutes(minutes))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// Sends the notices a stop lost, as a process that executes runs does
/// every minute (ADR-070, ADR-086); how many requests it told of.
pub async fn catch_up(api: &TestApi) -> usize {
    let runs = &api.app.services.runs;
    runs.start_executing().unwrap();
    let lost = runs.catch_up_notices().await.unwrap();
    api.idle().await;
    lost
}

/// A paused run of the other company, with a request due in `expires_in`
/// that its contact decides as manager: the run and the request.
pub async fn foreign_request(api: &TestApi, expires_in: Duration) -> (Uuid, Uuid) {
    let run = stopped_run(
        api,
        OTHER_TENANT_ID,
        OTHER_USER_ID,
        None,
        Some(saved()),
        &[],
    )
    .await;
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query("update agent_runs set status = 'awaiting_approval' where id = $1")
        .bind(run)
        .execute(&mut *tx)
        .await
        .unwrap();
    let request = create_approval(
        &mut tx,
        NewApproval {
            tenant_id: OTHER_TENANT_ID,
            run_id: run,
            requested_for_user_id: OTHER_USER_ID,
            tool_name: "planning_apply".into(),
            tool_call_id: "call-foreign".into(),
            action_summary: "Appliquer un planning".into(),
            payload: json!({}),
            risk: "write".into(),
            reason: "Action modifiant des données".into(),
            expires_at: Some(Utc::now() + expires_in),
            routing: ApprovalRouting::Manager(OTHER_USER_ID),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (run, request.id)
}

/// Ends the run of `foreign_request`, its request cancelled.
pub async fn end_foreign_request(api: &TestApi, run: Uuid, request: Uuid) {
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query("update agent_runs set status = 'failed' where id = $1")
        .bind(run)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("update approvals set status = 'cancelled' where id = $1")
        .bind(request)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// Takes `permission` from `user_id`; returns the policy to delete.
pub async fn without(paul: &Client<'_>, user_id: Uuid, permission: &str) -> String {
    let only =
        json!({ "op": "eq", "left": { "attr": "subject.userId" }, "right": { "value": user_id } });
    let created = paul
        .post(
            "/admin/policies",
            json!({ "description": "Droit retiré", "permissions": [permission], "condition": only }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    format!("/admin/policies/{}", created.json()["id"].as_str().unwrap())
}

/// Who did, and for whom, each `action` audited for the run, in order.
pub async fn audited(
    api: &TestApi,
    run_id: &str,
    action: &str,
) -> Vec<(Option<Uuid>, Option<Uuid>)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "select actor_user_id, on_behalf_of_user_id from audit_log \
         where run_id = $1::uuid and action = $2 order by id",
    )
    .bind(run_id)
    .bind(action)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows
}
