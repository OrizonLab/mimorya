//! An MCP server a company adds: the demo documentation server over HTTP on
//! a local port (reachable when the operator allows the private network).

use axum::http::StatusCode;
use serde_json::{Value, json};

use super::Client;
use super::llm::MASTER_KEY;

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
