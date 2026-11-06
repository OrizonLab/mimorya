//! A connector slow to start (ADR-177): the demo documentation server held
//! at its first request until the test lets it answer, its search mapped as
//! a read, on a server whose runs a person awaits wait for it one second,
//! its start lasting up to ten.

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_connectors::testing::{Hold, serve_held_mcp_http};
use mimorya_core::demo::tenant;
use serde_json::{Value, json};
use uuid::Uuid;

use super::llm::MASTER_KEY;
use super::mcp::{add, mcp_body};
use super::{Client, TestApi};

/// A server whose runs wait a second for a connector starting, which may
/// take ten.
pub const PATIENT: [(&str, &str); 4] = [
    ("MIMORYA_MASTER_KEY", MASTER_KEY),
    ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true"),
    ("MIMORYA_MCP_START_TIMEOUT_SECS", "10"),
    ("MIMORYA_MCP_START_PATIENCE_SECS", "1"),
];

/// The slow connector's search.
pub const SLOW_TOOL: &str = "mcp_lent_search_procedures";

/// The slow connector « Lent », added by `admin`, its search a read: its
/// id, and its server's hold, holding the next request.
pub async fn slow_connector(admin: &Client<'_>) -> (String, Arc<Hold>) {
    let (url, hold) = serve_held_mcp_http().await;
    // Answering while it is added and mapped.
    hold.release();
    let id = add(admin, mcp_body("Lent", "lent", &url, None)).await;
    let read = json!({ "risk": "read", "permissions": ["intervention:read"] });
    let mapped = admin
        .patch(
            &format!("/admin/connectors/{id}"),
            json!({ "toolPolicies": { SLOW_TOOL: read } }),
        )
        .await;
    assert_eq!(mapped.status, StatusCode::OK, "{}", mapped.text());
    hold.hold_again();
    (id, hold)
}

/// The model searches the slow connector, then answers `answer`.
pub fn searching(answer: &str) -> Vec<ScriptedStep> {
    let search = call(SLOW_TOOL, json!({ "query": "circulateur" }));
    vec![
        ScriptedStep::calls("", vec![search]),
        ScriptedStep::text(answer),
    ]
}

/// Lets the server answer two seconds after a start reached it: later
/// than the patience.
pub async fn answering_late(hold: &Hold) {
    hold.holding().await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    hold.release();
}

/// The run of the company started last.
pub async fn last_run(api: &TestApi) -> Uuid {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar("select id from agent_runs order by created_at desc limit 1")
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

/// What became of the run's search of the slow connector, as `client`
/// reads the run; `Null` if it made none.
pub async fn search_status(client: &Client<'_>, run: Uuid) -> Value {
    let detail = client.get(&format!("/runs/{run}")).await.json();
    let calls = detail["tools"].as_array().cloned().unwrap_or_default();
    calls
        .iter()
        .find(|call| call["toolName"] == SLOW_TOOL)
        .map(|call| call["status"].clone())
        .unwrap_or(Value::Null)
}

/// Deletes the slow connector, as `admin`.
pub async fn remove(admin: &Client<'_>, id: &str) {
    let deleted = admin.delete(&format!("/admin/connectors/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}
