//! A call plays the rounds its server asks for (`input_required`, SEP-2322)
//! within its delay (ADR-164): a server that answers a first round with its
//! state only, for the call to be made again, then answers it. A server
//! that asks for more rounds than a call plays has answered: no ping
//! follows (ADR-165).

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use mimorya_connectors::ConnectorRuntime;
use mimorya_connectors::mcp::{McpToolsetOptions, create_mcp_toolset};
use rmcp::ServiceExt;
use serde_json::{Value, json};
use support::mcp::call_as_reader;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

/// A server written out by hand, line by line: the MCP library's own
/// server sends a round only under a protocol version it does not
/// negotiate here. With `endless`, every call asks for one more round. It
/// counts the pings it is sent.
async fn serve_rounds(io: DuplexStream, endless: bool, pings: Arc<AtomicUsize>) {
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
        let again = message["params"]["requestState"] == "again" && !endless;
        let mut answer = match message["method"].as_str() {
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
            Some("tools/call") if again => {
                json!({ "result": { "content": [{ "type": "text", "text": "rejoué" }] } })
            }
            Some("tools/call") => {
                json!({ "result": { "resultType": "input_required", "requestState": "again" } })
            }
            Some("ping") => {
                pings.fetch_add(1, Ordering::SeqCst);
                json!({ "result": {} })
            }
            _ => json!({ "error": { "code": -32601, "message": "Méthode inconnue" } }),
        };
        answer["jsonrpc"] = json!("2.0");
        answer["id"] = id;
        let mut line = answer.to_string();
        line.push('\n');
        if writing.write_all(line.as_bytes()).await.is_err() {
            return;
        }
    }
}

/// An instance of the server, and the pings it is sent.
async fn start(endless: bool) -> (ConnectorRuntime, Arc<AtomicUsize>) {
    let pings = Arc::new(AtomicUsize::new(0));
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    tokio::spawn(serve_rounds(server_io, endless, pings.clone()));
    let client = Arc::new(().serve(client_io).await.unwrap());
    let tools = create_mcp_toolset(client, &McpToolsetOptions::new("docs"))
        .await
        .unwrap();
    let runtime = ConnectorRuntime {
        tools,
        closer: None,
    };
    (runtime, pings)
}

#[tokio::test]
async fn a_call_plays_the_rounds_its_server_asks_for() {
    let (runtime, _) = start(false).await;
    let result = call_as_reader(&runtime, "mcp_docs_retried_item", json!({})).await;
    let said = result.content.to_string();
    assert!(!result.is_error && said.contains("rejoué"), "{said}");
}

#[tokio::test]
async fn more_rounds_than_a_call_plays_are_an_answer_and_start_no_ping() {
    let (runtime, pings) = start(true).await;
    let result = call_as_reader(&runtime, "mcp_docs_retried_item", json!({})).await;
    assert!(result.is_error, "{}", result.content);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(pings.load(Ordering::SeqCst), 0);
}
