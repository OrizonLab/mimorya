//! A call plays the rounds its server asks for (`input_required`, SEP-2322)
//! within its delay (ADR-164): a server that answers a first round with its
//! state only, for the call to be made again, then answers it.

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::{ToolCallEvaluation, ToolOverride, ToolRegistry};
use mimorya_connectors::mcp::{McpToolsetOptions, create_mcp_toolset};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use rmcp::ServiceExt;
use serde_json::{Value, json};
use support::executor_for;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

/// A server written out by hand, line by line: the MCP library's own
/// server sends a round only under a protocol version it does not
/// negotiate here.
async fn serve_rounds(io: DuplexStream) {
    let (reading, mut writing) = tokio::io::split(io);
    let mut lines = BufReader::new(reading).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        // Notifications are not answered.
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let answer = match message["method"].as_str() {
            Some("initialize") => json!({ "result": {
                "protocolVersion": message["params"]["protocolVersion"],
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "rounds", "version": "0.1.0" },
            }}),
            Some("tools/list") => json!({ "result": { "tools": [{
                "name": "retried_item",
                "description": "Lit un élément.",
                "inputSchema": { "type": "object" },
            }]}}),
            Some("tools/call") if message["params"]["requestState"] == "again" => {
                json!({ "result": { "content": [{ "type": "text", "text": "rejoué" }] } })
            }
            Some("tools/call") => {
                json!({ "result": { "resultType": "input_required", "requestState": "again" } })
            }
            Some("ping") => json!({ "result": {} }),
            _ => json!({ "error": { "code": -32601, "message": "Méthode inconnue" } }),
        };
        let mut answer = answer;
        answer["jsonrpc"] = json!("2.0");
        answer["id"] = id;
        let mut line = answer.to_string();
        line.push('\n');
        if writing.write_all(line.as_bytes()).await.is_err() {
            return;
        }
    }
}

#[tokio::test]
async fn a_call_plays_the_rounds_its_server_asks_for() {
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    tokio::spawn(serve_rounds(server_io));
    let client = Arc::new(().serve(client_io).await.unwrap());
    let tools = create_mcp_toolset(client, &McpToolsetOptions::new("docs"))
        .await
        .unwrap();
    let read = ToolOverride {
        risk: Some(ToolRisk::Read),
        permissions: Some(vec!["intervention:read".into()]),
        ..Default::default()
    };
    let overrides = HashMap::from([("mcp_docs_retried_item".to_string(), read)]);
    let registry = ToolRegistry::new(tools, &overrides).unwrap();
    let ports = InMemoryPorts::new(user::LUCAS, uuid::Uuid::new_v4());
    let lucas = executor_for(registry, user::LUCAS, &ports);
    let call = LlmToolCall {
        id: "1".into(),
        name: "mcp_docs_retried_item".into(),
        input: json!({}),
    };
    let ToolCallEvaluation::Allow { tool, input } = lucas.evaluate(&call) else {
        panic!("expected the mapped tool to run directly");
    };
    let (result, _) = lucas.execute(&tool, input).await;
    let said = result.content.to_string();
    assert!(!result.is_error && said.contains("rejoué"), "{said}");
}
