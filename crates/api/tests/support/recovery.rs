//! Runs a stop of the service left running (ADR-053), as the database holds
//! them, and taking them up as the API does at startup.

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::{DEFAULT_BUDGET, RunCheckpoint, RunEvent};
use mimorya_core::autonomy::{AutonomyLevel, ToolRisk};
use mimorya_core::demo::tenant;
use mimorya_db::repositories::run_events::{append_run_event, list_run_events};
use mimorya_db::repositories::runs::{NewRun, RunPatch, create_run, get_run, update_run};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, TestApi, owner_url};

/// The title and risk the demo connector declares for its tools.
fn demo_tool(tool: &str) -> (&str, Option<ToolRisk>) {
    match tool {
        "interventions_search" => ("Recherche d’interventions", Some(ToolRisk::Read)),
        "planning_propose" => ("Proposition de planning", Some(ToolRisk::Draft)),
        "planning_apply" => ("Application d’un planning", Some(ToolRisk::Write)),
        other => (other, None),
    }
}

/// A run of `user` (in `conversation`, if any) left running by a stop of the
/// service at `state`, after it started the tools `started` (calls allowed,
/// as the engine reports them, with their risk).
pub async fn stopped_run(
    api: &TestApi,
    tenant_id: Uuid,
    user_id: Uuid,
    conversation: Option<Uuid>,
    state: Option<RunCheckpoint>,
    started: &[&str],
) -> Uuid {
    let state = state.map(|s| serde_json::to_value(s).unwrap());
    stopped_with(api, tenant_id, user_id, conversation, state, started).await
}

/// Same as `stopped_run`, its state saved as given (unreadable, say).
pub async fn stopped_with(
    api: &TestApi,
    tenant_id: Uuid,
    user_id: Uuid,
    conversation: Option<Uuid>,
    state: Option<Value>,
    started: &[&str],
) -> Uuid {
    let db = &api.app.services.db;
    let mut tx = db.begin(tenant_id).await.unwrap();
    let run = create_run(
        &mut tx,
        NewRun {
            tenant_id,
            conversation_id: conversation,
            initiated_by_user_id: user_id,
            on_behalf_of_user_id: user_id,
            objective: "Fais le point sur la semaine".into(),
            success_criteria: Vec::new(),
            autonomy_level: AutonomyLevel::ExecuteWithApproval,
            budget: serde_json::to_value(DEFAULT_BUDGET).unwrap(),
            request_message_id: None,
        },
    )
    .await
    .unwrap();
    if let Some(state) = state {
        let patch = RunPatch {
            state: Some(state),
            ..RunPatch::default()
        };
        update_run(&mut tx, run.id, patch).await.unwrap();
    }
    tx.commit().await.unwrap();
    for (n, tool) in started.iter().enumerate() {
        let (title, risk) = demo_tool(tool);
        let (kind, data) = RunEvent::ToolCall {
            tool_call_id: format!("c{n}"),
            tool_name: (*tool).into(),
            title: title.into(),
            decision: "allow".into(),
            reason: None,
            risk,
        }
        .into_parts();
        let mut tx = db.begin(tenant_id).await.unwrap();
        append_run_event(&mut tx, tenant_id, run.id, &kind, &data)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    run.id
}

/// Merges `policies` into the demo connector's tool policies, as its
/// administrator would set them; returns those it had.
pub async fn set_demo_policies(api: &TestApi, policies: Value) -> Value {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let before: Value = sqlx::query_scalar(
        "select tool_policies from connector_instances where type = 'demo_interventions'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "update connector_instances set tool_policies = tool_policies || $1 \
         where type = 'demo_interventions'",
    )
    .bind(policies)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    before
}

/// Gives the demo connector its tool policies back.
pub async fn restore_demo_policies(api: &TestApi, policies: Value) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update connector_instances set tool_policies = $1 where type = 'demo_interventions'",
    )
    .bind(policies)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// The run as saved before its first model call.
pub fn saved() -> RunCheckpoint {
    RunCheckpoint::new(
        "CONSIGNES ENREGISTRÉES".into(),
        vec![LlmMessage::user("Fais le point sur la semaine")],
    )
}

/// As if the run had been interrupted that long ago.
pub async fn interrupted_ago(run_id: Uuid, minutes: i32) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(
        "update agent_runs set updated_at = now() - make_interval(mins => $2) where id = $1",
    )
    .bind(run_id)
    .bind(minutes)
    .execute(&owner)
    .await
    .unwrap();
}

pub async fn conversation_of(client: &Client<'_>) -> Uuid {
    let created = client.post("/conversations", json!({})).await.json();
    created["conversation"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Takes up the stopped runs, as at startup, and waits for them.
pub async fn restart(api: &TestApi) -> usize {
    let taken = api.app.recover_interrupted_runs().await.unwrap();
    api.idle().await;
    taken
}

pub async fn status_and_error(
    api: &TestApi,
    tenant_id: Uuid,
    run_id: Uuid,
) -> (String, Option<String>) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let run = get_run(&mut tx, run_id).await.unwrap().unwrap();
    (run.status, run.error)
}

pub async fn events(api: &TestApi, tenant_id: Uuid, run_id: Uuid) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    list_run_events(&mut tx, run_id, 0)
        .await
        .unwrap()
        .into_iter()
        .map(|e| (e.event_type, e.data))
        .collect()
}

pub async fn last_answer(client: &Client<'_>, conversation: Uuid) -> String {
    let thread = client
        .get(&format!("/conversations/{conversation}"))
        .await
        .json();
    let messages = thread["messages"].as_array().unwrap();
    messages.last().unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned()
}
