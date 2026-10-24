//! Missions (ADR-089): entrusted through the API, their stages, each queued,
//! taken from the queue one at a time by the test, and what a stage that
//! spends its budget asks of the model.

use axum::http::StatusCode;
use mimorya_agent::runtime::MISSION_STAGE_BUDGET;
use mimorya_agent::testing::{ScriptedStep, call};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, TestApi, owner_url, week};

pub const OBJECTIVE: &str = "Relis les interventions de la semaine et propose leur suivi.";
pub const CRITERION: &str = "Nommer chaque intervention suivie";

/// The process executes runs: its stages are taken from the queue by
/// `take_queued`.
pub fn executing(api: &TestApi) {
    api.app.services.runs.start_executing().unwrap();
}

pub fn mission_body(title: &str) -> Value {
    json!({ "title": title, "objective": OBJECTIVE, "successCriteria": [CRITERION] })
}

/// Entrusts a mission named `title` to `client`'s agent: its identifier.
pub async fn entrust(client: &Client<'_>, title: &str) -> Uuid {
    let entrusted = client.post("/missions", mission_body(title)).await;
    assert_eq!(
        entrusted.status,
        StatusCode::CREATED,
        "{}",
        entrusted.text()
    );
    entrusted.json()["id"].as_str().unwrap().parse().unwrap()
}

/// Stops the mission, as `client`: the answer's status.
pub async fn stop(client: &Client<'_>, id: Uuid) -> StatusCode {
    client
        .post(&format!("/missions/{id}/stop"), json!({}))
        .await
        .status
}

/// The mission, as its owner reads it.
pub async fn read(client: &Client<'_>, id: Uuid) -> Value {
    let read = client.get(&format!("/missions/{id}")).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    read.json()
}

/// The messages of the mission's conversation.
pub async fn messages(client: &Client<'_>, mission: &Value) -> Vec<Value> {
    let conversation = mission["conversationId"].as_str().unwrap();
    let detail = client.get(&format!("/conversations/{conversation}")).await;
    detail.json()["messages"].as_array().unwrap().clone()
}

/// A stage that searches the week's interventions under way (INT-2049
/// among them) until its budget is spent, then writes `note`.
pub fn spending_stage(note: &str) -> Vec<ScriptedStep> {
    let mut search = week();
    search["status"] = json!(["in_progress"]);
    let mut steps: Vec<ScriptedStep> = (0..MISSION_STAGE_BUDGET.max_steps)
        .map(|_| ScriptedStep::calls("", vec![call("interventions_search", search.clone())]))
        .collect();
    steps.push(ScriptedStep::text(note));
    steps
}

/// The model's review of a final answer: every criterion met.
pub fn approving_review() -> ScriptedStep {
    ScriptedStep::calls(
        "",
        vec![call(
            "submit_verdict",
            json!({ "pass": true, "issues": [] }),
        )],
    )
}

/// Sets the mission's own limits, as when it was entrusted.
pub async fn limit(mission: Uuid, max_stages: u32, max_cost_micros: i64) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let limits = json!({ "maxStages": max_stages, "maxCostMicros": max_cost_micros });
    sqlx::query("update missions set limits = $2 where id = $1")
        .bind(mission)
        .bind(limits)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

/// The runs of the mission, oldest first: identifier and status.
pub async fn stages(mission: Uuid) -> Vec<(Uuid, String)> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let stages = sqlx::query_as(
        "select id, status from agent_runs where mission_id = $1 order by created_at, id",
    )
    .bind(mission)
    .fetch_all(&owner)
    .await
    .unwrap();
    owner.close().await;
    stages
}

/// The stage's cost budget, as it was given.
pub async fn cost_budget(run: Uuid) -> i64 {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let budget: i64 = sqlx::query_scalar(
        "select (budget->>'maxCostMicros')::bigint from agent_runs where id = $1",
    )
    .bind(run)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    budget
}
