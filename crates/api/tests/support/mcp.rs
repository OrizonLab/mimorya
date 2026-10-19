//! An MCP server a company adds: the demo documentation server over HTTP on
//! a local port (reachable when the operator allows the private network),
//! and what the server logs meanwhile.

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use mimorya_api::log_filter;
use mimorya_core::demo::tenant;
use serde_json::{Value, json};
use uuid::Uuid;

use super::llm::MASTER_KEY;
use super::{Client, TestApi};

pub const TOKEN: &str = "jeton-mcp-de-test-0123456789";
pub const TOOL: &str = "mcp_docs_search_procedures";

/// A server allowing the private network (the local MCP server) and storing secrets.
pub const PRIVATE_DEPLOYMENT: [(&str, &str); 2] = [
    ("MIMORYA_MASTER_KEY", MASTER_KEY),
    ("MIMORYA_MCP_ALLOW_PRIVATE_NETWORK", "true"),
];

pub fn mcp_body(name: &str, prefix: &str, url: &str, token: Option<&str>) -> Value {
    let mut body = json!({ "name": name, "prefix": prefix, "url": url });
    if let Some(token) = token {
        body["token"] = json!(token);
    }
    body
}

/// Adds the connector and returns its id.
pub async fn add(admin: &Client<'_>, body: Value) -> String {
    let created = admin.post("/admin/connectors", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_string()
}

/// The instance as listed to its administrators.
pub async fn listed(admin: &Client<'_>, id: &str) -> Value {
    admin.get("/admin/connectors").await.json()["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id)
        .cloned()
        .unwrap_or(Value::Null)
}

/// What the server logs on this thread while kept, through its own filter.
pub struct CapturedLogs {
    buffer: Arc<Mutex<Vec<u8>>>,
    _guard: tracing::subscriber::DefaultGuard,
}

impl CapturedLogs {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.buffer.lock().expect("logs")).into_owned()
    }
}

struct LogWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("logs").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Captures the logs of the current thread (the test runtime runs every
/// task on it) with the filter the server uses at the default level.
pub fn capture_logs() -> CapturedLogs {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let shared = buffer.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(log_filter("info").expect("filter"))
        .with_writer(move || LogWriter(shared.clone()))
        .with_ansi(false)
        .finish();
    CapturedLogs {
        buffer,
        _guard: tracing::subscriber::set_default(subscriber),
    }
}

/// Writes the policies of an instance as the operator would, in the database.
pub async fn write_policies(api: &TestApi, id: &str, policies: Value) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update connector_instances set tool_policies = $2, updated_at = now() where id = $1",
    )
    .bind(Uuid::parse_str(id).unwrap())
    .bind(policies)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}
