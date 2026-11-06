//! A run waits for a connector still starting no longer than its patience
//! (ADR-177): it goes without it, told the connector still starts, and
//! nothing is recorded as a failure; the start goes on in the background
//! and serves the next run.

mod support;

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use mimorya_api::services::connectors::STILL_STARTING;
use mimorya_connectors::testing::serve_held_mcp_http;
use mimorya_core::demo::{tenant, user};
use support::TestApi;
use support::connector_sessions::RUN;
use support::llm::MASTER_KEY;
use support::mcp::{add, listed, mcp_body};

/// A server whose runs wait a second for a connector starting, which may
/// take ten.
const PATIENT: [(&str, &str); 4] = [
    ("MIMORYA_MASTER_KEY", MASTER_KEY),
    ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true"),
    ("MIMORYA_MCP_START_TIMEOUT_SECS", "10"),
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
    let tools = connectors.tools_for(tenant::ID, RUN).await.unwrap();
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
