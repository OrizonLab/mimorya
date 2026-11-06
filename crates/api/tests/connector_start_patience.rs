//! A run whose first answer a person waits for waits for a connector still
//! starting no longer than its patience (ADR-177): it goes without it,
//! told the connector still starts, and nothing is recorded as a failure;
//! the start goes on in the background and serves the next run, or, if
//! it fails, is logged and recorded where it fails, no run reading it. A
//! person's turn, through the executor, goes on without it, named to them
//! as unavailable. A run resumed to do an action a person validated has no
//! patience: it waits for its connector however long it takes to start
//! again. The runs no one awaits: `connector_start_waits.rs`.

mod support;

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_api::services::connectors::STILL_STARTING;
use mimorya_connectors::testing::serve_held_mcp_http;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::TestApi;
use support::connector_sessions::RUN;
use support::llm::MASTER_KEY;
use support::mcp::{add, capture_logs, listed, mcp_body};
use support::runs::start_run;
use support::slow_starts::{PATIENT, remove, slow_connector};

/// The same, a start lasting two seconds at most.
const SHORT_START: [(&str, &str); 4] = [
    ("MIMORYA_MASTER_KEY", MASTER_KEY),
    ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true"),
    ("MIMORYA_MCP_START_TIMEOUT_SECS", "2"),
    ("MIMORYA_MCP_START_PATIENCE_SECS", "1"),
];

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_goes_without_a_connector_still_starting() {
    let api = TestApi::start_with(&PATIENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let (held, hold) = serve_held_mcp_http().await;
    let id = add(&paul, mcp_body("Lent", "lent", &held, None)).await;
    let connectors = &api.app.services.connectors;

    // Still starting once the run's patience is over: the run goes on.
    let started = Instant::now();
    let tools = connectors
        .tools_within_patience(tenant::ID, RUN)
        .await
        .unwrap();
    let waited = started.elapsed();
    assert!(waited >= Duration::from_secs(1), "{waited:?}");
    assert!(waited < Duration::from_secs(5), "{waited:?}");
    let [issue] = tools.issues.as_slice() else {
        panic!("one connector unavailable: {:?}", tools.issues);
    };
    assert_eq!(issue.connector, "Lent");
    assert_eq!(issue.message, STILL_STARTING);
    assert!(!issue.to_authorize);
    // No failure: neither recorded nor shown to its administrators.
    let instance = listed(&paul, &id).await;
    assert_eq!(instance["status"], "untested", "{instance}");
    assert_eq!(instance["lastError"], serde_json::Value::Null);

    // The start went on: once the server answers, it serves the next run.
    hold.reached().await;
    hold.release();
    let tools = connectors.tools_for(tenant::ID, RUN).await.unwrap();
    assert!(tools.issues.is_empty(), "{:?}", tools.issues);
    let lent = tools
        .tools
        .iter()
        .filter(|tool| tool.info().name.starts_with("mcp_lent_"));
    assert!(lent.count() > 0, "its tools serve the run");
    assert_eq!(listed(&paul, &id).await["status"], "ok");
    let deleted = paul.delete(&format!("/admin/connectors/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_s_turn_goes_on_without_a_connector_still_starting() {
    let api = TestApi::start_with(&PATIENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let (id, hold) = slow_connector(&paul).await;
    api.script(vec![ScriptedStep::text(
        "Les procédures ne sont pas encore disponibles.",
    )]);
    let started = Instant::now();
    let run = start_run(&marc).await;
    api.idle().await;
    let waited = started.elapsed();
    hold.release();
    // Its patience, not the connector's start.
    assert!(waited < Duration::from_secs(5), "{waited:?}");
    let stream = marc.get(&format!("/runs/{run}/events")).await.text();
    let announced = stream
        .split("\n\n")
        .find(|event| event.contains("event: connectors_unavailable"))
        .unwrap_or_default();
    assert!(announced.contains("\"Lent\""), "{stream}");
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_validated_call_waits_for_its_connector_however_long_it_starts() {
    let api = TestApi::start_with(&PATIENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let (held, hold) = serve_held_mcp_http().await;
    hold.release();
    // Unmapped: a write, validated by whom the agent acts for.
    let id = add(&paul, mcp_body("Lent", "lent", &held, None)).await;
    let tool = "mcp_lent_search_procedures";
    let input = json!({ "query": "circulateur" });
    api.script(vec![ScriptedStep::calls("", vec![call(tool, input)])]);
    let conversation = marc.post("/conversations", json!({})).await.json();
    let conversation_id = conversation["conversation"]["id"].as_str().unwrap();
    let started = marc
        .post(
            &format!("/conversations/{conversation_id}/messages"),
            json!({ "content": "Cherche la procédure du circulateur." }),
        )
        .await;
    let run_id = started.json()["runId"].as_str().unwrap().to_string();
    api.idle().await;
    let approvals = marc.get("/approvals?status=pending").await.json();
    let pending = &approvals["approvals"][0];
    assert_eq!(pending["toolName"], tool, "{approvals}");

    // Validated once the instance is gone and its server slower to start
    // again than the patience: the resumed run waits, then does it.
    let connectors = &api.app.services.connectors;
    connectors.invalidate(id.parse().unwrap()).await;
    hold.hold_again();
    api.script(vec![ScriptedStep::text("Voici la procédure.")]);
    let decision = format!("/approvals/{}/decision", pending["id"].as_str().unwrap());
    let approved = marc.post(&decision, json!({ "approved": true }));
    let answering = async {
        tokio::time::sleep(Duration::from_secs(2)).await;
        hold.release();
    };
    let (approved, ()) = tokio::join!(approved, answering);
    assert_eq!(approved.status, StatusCode::OK, "{}", approved.text());
    api.idle().await;
    let detail = marc.get(&format!("/runs/{run_id}")).await.json();
    let ran = detail["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["toolName"] == tool && t["status"] == "succeeded");
    assert!(ran, "{detail}");
    let deleted = paul.delete(&format!("/admin/connectors/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_start_failing_in_the_background_is_logged_and_recorded() {
    let api = TestApi::start_with(&SHORT_START).await;
    let paul = api.sign_in(user::PAUL).await;
    // Never answers: its start fails once the start's own time is over.
    let (held, _hold) = serve_held_mcp_http().await;
    let id = add(&paul, mcp_body("Muet", "muet", &held, None)).await;
    let logs = capture_logs();
    let connectors = &api.app.services.connectors;
    let tools = connectors
        .tools_within_patience(tenant::ID, RUN)
        .await
        .unwrap();
    assert_eq!(tools.issues[0].message, STILL_STARTING);

    // No run reads it: recorded on the connector all the same, and logged.
    let recorded = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let instance = listed(&paul, &id).await;
            if instance["status"] == "error" {
                return instance;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("recorded as failing");
    assert!(recorded["lastError"].as_str().is_some(), "{recorded}");
    let logged = logs.text();
    drop(logs);
    assert!(logged.contains("Connector unavailable"), "{logged}");
    let deleted = paul.delete(&format!("/admin/connectors/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK);
}
