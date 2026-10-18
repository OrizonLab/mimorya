//! The conversation → run → approval → resume cycle with a scripted model,
//! including authorization of other users and SSE replay.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::planning::{apply_proposal, propose};
use support::{TestApi, week};

fn ids(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn refuses_to_start_a_run_when_no_model_provider_is_configured() {
    let api = TestApi::without_llm().await;
    let marc = api.sign_in(user::MARC).await;
    let conversation = marc.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let response = marc
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": "Bonjour" }),
        )
        .await;
    assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(!api.app.services.runs.llm_available());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn runs_a_turn_pauses_for_validation_and_resumes_after_the_decision() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let julien = api.sign_in(user::JULIEN).await;
    let mut search = week();
    search["status"] = json!(["in_progress"]);
    api.script(vec![
        ScriptedStep::calls("", vec![call("interventions_search", search)]),
        propose("INT-2049", "13:00"),
        apply_proposal("Je demande la validation."),
        ScriptedStep::text("Planning appliqué : suivi de INT-2049 programmé."),
    ]);

    let conversation = marc.post("/conversations", json!({})).await;
    assert_eq!(conversation.status, StatusCode::CREATED);
    let conversation_id = conversation.json()["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let messages_url = format!("/conversations/{conversation_id}/messages");
    let started = marc
        .post(
            &messages_url,
            json!({ "content": "Propose un suivi pour les interventions en cours" }),
        )
        .await;
    assert_eq!(started.status, StatusCode::ACCEPTED);
    let run_id = started.json()["runId"].as_str().unwrap().to_string();
    api.idle().await;

    let pending = marc.get("/approvals").await.json();
    let approvals = pending["approvals"].as_array().unwrap();
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0]["toolName"], "planning_apply");
    let decision_url = format!(
        "/approvals/{}/decision",
        approvals[0]["id"].as_str().unwrap()
    );
    let run_url = format!("/runs/{run_id}");
    assert_eq!(
        marc.get(&run_url).await.json()["run"]["status"],
        "awaiting_approval"
    );

    // A second request cannot start while the run waits in this conversation.
    let concurrent = marc
        .post(&messages_url, json!({ "content": "Autre demande" }))
        .await;
    assert_eq!(concurrent.status, StatusCode::CONFLICT);

    // Nobody else can validate an action requested on Marc's behalf.
    let foreign = julien
        .post(&decision_url, json!({ "approved": true }))
        .await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);

    let decision = marc.post(&decision_url, json!({ "approved": true })).await;
    assert_eq!(decision.status, StatusCode::OK);
    assert_eq!(decision.json()["resumed"], true);
    api.idle().await;
    let again = marc.post(&decision_url, json!({ "approved": false })).await;
    assert_eq!(again.status, StatusCode::CONFLICT);

    let detail = marc.get(&run_url).await.json();
    assert_eq!(detail["run"]["status"], "completed");
    let tools: Vec<String> = detail["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            format!(
                "{}:{}",
                t["toolName"].as_str().unwrap(),
                t["status"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        tools,
        [
            "interventions_search:succeeded",
            "planning_propose:succeeded",
            "planning_apply:awaiting_approval",
            "planning_apply:succeeded",
        ]
    );

    let thread = marc
        .get(&format!("/conversations/{conversation_id}"))
        .await
        .json();
    let roles: Vec<&str> = thread["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["user", "assistant"]);
    assert!(
        thread["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("INT-2049")
    );
    assert!(
        ids(&marc.get("/conversations").await.json(), "conversations").contains(&conversation_id)
    );
    assert!(ids(&marc.get("/runs").await.json(), "runs").contains(&run_id));

    // Other users cannot see Marc's conversations or runs.
    let julien_thread = julien
        .get(&format!("/conversations/{conversation_id}"))
        .await;
    assert_eq!(julien_thread.status, StatusCode::NOT_FOUND);
    assert_eq!(julien.get(&run_url).await.status, StatusCode::NOT_FOUND);
    let julien_stream = julien.get(&format!("{run_url}/events")).await;
    assert_eq!(julien_stream.status, StatusCode::NOT_FOUND);

    // The event stream replays persisted events and ends with the run.
    let stream = marc.get(&format!("{run_url}/events")).await;
    let content_type = stream.headers["content-type"].to_str().unwrap();
    assert!(content_type.contains("text/event-stream"));
    let body = stream.text();
    assert!(body.contains("event: approval_requested"));
    assert!(body.contains("event: run_finished"));
    assert!(body.trim().ends_with(r#""status":"completed"}"#), "{body}");

    let from_end = marc
        .get_with(&format!("{run_url}/events"), &[("last-event-id", "1000")])
        .await;
    assert_eq!(from_end.text(), "");
    let from_query = marc.get(&format!("{run_url}/events?after=1000")).await;
    assert_eq!(from_query.text(), "");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn opens_a_single_turn_when_two_messages_arrive_together() {
    let api = TestApi::start().await;
    api.script(vec![ScriptedStep::text("Bien reçu.")]);
    let marc = api.sign_in(user::MARC).await;
    let conversation = marc.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let url = format!("/conversations/{id}/messages");
    let (first, second) = tokio::join!(
        marc.post(&url, json!({ "content": "Première demande" })),
        marc.post(&url, json!({ "content": "Seconde demande" })),
    );
    let mut statuses = [first.status, second.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::ACCEPTED, StatusCode::CONFLICT]);
    api.idle().await;

    let thread = marc.get(&format!("/conversations/{id}")).await.json();
    assert_eq!(thread["messages"].as_array().unwrap().len(), 2);
    assert_eq!(thread["runs"].as_array().unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn keeps_other_users_out_of_a_conversation() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let julien = api.sign_in(user::JULIEN).await;
    let conversation = marc.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let url = format!("/conversations/{id}");

    let rename = julien.patch(&url, json!({ "title": "Détourné" })).await;
    assert_eq!(rename.status, StatusCode::NOT_FOUND);
    let message = julien
        .post(&format!("{url}/messages"), json!({ "content": "Bonjour" }))
        .await;
    assert_eq!(message.status, StatusCode::NOT_FOUND);

    let renamed = marc.patch(&url, json!({ "title": "  Suivi Nord  " })).await;
    assert_eq!(renamed.status, StatusCode::OK);
    assert_eq!(
        marc.get(&url).await.json()["conversation"]["title"],
        "Suivi Nord"
    );
    assert_eq!(
        marc.patch(&url, json!({ "archived": true })).await.status,
        StatusCode::OK
    );
    let listed = marc.get("/conversations").await.json();
    assert!(!ids(&listed, "conversations").contains(&id));
}
