//! Lessons drawn from runs (ADR-076): the company's choice, runs that use
//! tools (and the rereading's steps after them), the lessons stored, and
//! what the model was asked last.

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_core::demo::tenant;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, TestApi, week};

pub const ANSWER: &str = "Une intervention reste en cours : INT-2049.";

/// Whether a lesson is drawn from the company's runs.
pub async fn capture_lessons(paul: &Client<'_>, enabled: bool) {
    let body = json!({ "captureLessons": enabled });
    let set = paul.put("/admin/memory-settings", body).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
}

/// A turn of `client` that searches the interventions, then the rereading's
/// steps; the run's identifier.
pub async fn run_with_tools(api: &TestApi, client: &Client<'_>, reread: Vec<ScriptedStep>) -> Uuid {
    let mut search = week();
    search["status"] = json!(["in_progress"]);
    let mut steps = vec![
        ScriptedStep::calls("", vec![call("interventions_search", search)]),
        ScriptedStep::text(ANSWER),
    ];
    steps.extend(reread);
    api.script(steps);
    ask(api, client).await
}

/// A turn of `client`, once the model's steps are scripted; its run.
pub async fn ask(api: &TestApi, client: &Client<'_>) -> Uuid {
    let conversation = client.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let sent = client
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": "Quelles interventions restent en cours ?" }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    api.idle().await;
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0, "steps left");
    sent.json()["runId"].as_str().unwrap().parse().unwrap()
}

/// The rereading's answer: a lesson so titled.
pub fn lesson(title: &str) -> ScriptedStep {
    ScriptedStep::calls(
        "",
        vec![call(
            "submit_lesson",
            json!({
                "aucune": false,
                "titre": title,
                "contenu": "Commencer par les interventions en cours avant de planifier.",
            }),
        )],
    )
}

/// The lessons drawn from runs, as stored: owner, status, title, run.
pub async fn lessons(api: &TestApi) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select json_build_object('id', id, 'owner', owner_user_id, 'scope', scope, \
         'status', status, 'title', title, 'run', created_by_run_id) \
         from memory_items where kind = 'lesson' and created_by_run_id is not null \
         order by created_at",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// Whether `client` finds the memory `id` among theirs.
pub async fn listed(client: &Client<'_>, id: &str) -> bool {
    let items = client.get("/memory").await.json()["items"].clone();
    items.as_array().unwrap().iter().any(|m| m["id"] == id)
}

/// The system prompt of the last request to the model.
pub fn last_system(api: &TestApi) -> String {
    let requests = api.model.as_ref().unwrap().requests();
    requests.last().unwrap().system.clone()
}
