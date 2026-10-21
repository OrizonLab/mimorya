//! Validation by a manager (ADR-059): the routing setting, a scripted run
//! asking to apply a planning, and what the lists and the audit say of it.

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::tenant;
use serde_json::{Value, json};
use uuid::Uuid;

use super::planning::{apply_proposal, propose};
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
    (intervention, start_time): (&str, &str),
    final_text: &str,
) -> (String, Option<Value>) {
    api.script(vec![
        propose(intervention, start_time),
        apply_proposal(""),
        ScriptedStep::text(final_text),
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
