//! A session with an MCP server kept between runs: an HTTP server that
//! restarts is reached again by the same session (the client opens a new
//! one when the old is unknown), and a local server (stdio) that exits
//! leaves a session that says it is closed, for the platform to start it
//! again.

mod support;

use std::collections::HashMap;
use std::time::Duration;

use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::{ToolCallEvaluation, ToolOverride, ToolRegistry};
use mimorya_connectors::mcp::NetworkPolicy;
use mimorya_connectors::testing::serve_stoppable_demo_docs_http;
use mimorya_connectors::{ConnectorRuntime, InstanceContext, connector_type};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::executor_for;

const TOOL: &str = "mcp_docs_search_procedures";

async fn start(config: Value, network: NetworkPolicy) -> ConnectorRuntime {
    connector_type("mcp")
        .unwrap()
        .instantiate(
            &config,
            InstanceContext {
                network,
                ..InstanceContext::default()
            },
        )
        .await
        .unwrap()
}

/// Whether a search through the session's tool succeeds.
async fn searched(runtime: &ConnectorRuntime) -> bool {
    let overrides = HashMap::from([(
        TOOL.to_string(),
        ToolOverride {
            risk: Some(ToolRisk::Read),
            permissions: Some(vec!["intervention:read".into()]),
            ..Default::default()
        },
    )]);
    let registry = ToolRegistry::new(runtime.tools.clone(), &overrides).unwrap();
    let ports = InMemoryPorts::new(user::LUCAS, uuid::Uuid::new_v4());
    let executor = executor_for(registry, user::LUCAS, &ports);
    let call = LlmToolCall {
        id: "1".into(),
        name: TOOL.into(),
        input: json!({ "query": "circulateur" }),
    };
    let ToolCallEvaluation::Allow { tool, input } = executor.evaluate(&call) else {
        panic!("expected the mapped tool to run");
    };
    !executor.execute(&tool, input).await.0.is_error
}

#[tokio::test]
async fn an_http_session_reaches_its_server_again_after_a_restart() {
    let (url, stop) = serve_stoppable_demo_docs_http(0).await;
    let port: u16 = url
        .trim_start_matches("http://127.0.0.1:")
        .split('/')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let private = NetworkPolicy {
        allow_private_network: true,
        allow_stdio: false,
    };
    let runtime = start(
        json!({ "transport": "http", "prefix": "docs", "url": url }),
        private,
    )
    .await;
    assert!(searched(&runtime).await);

    // Stopped: calls fail, but the session is not closed for good.
    stop.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!searched(&runtime).await);
    assert!(!runtime.is_closed());

    // Back, without the old session: the same runtime reaches it again.
    let (_, _running) = serve_stoppable_demo_docs_http(port).await;
    assert!(searched(&runtime).await);
    assert!(!runtime.is_closed());
    runtime.closer.unwrap().close().await;
}

#[tokio::test]
async fn a_local_server_that_exits_leaves_a_session_that_says_it_is_closed() {
    // The demo server, ended after a second.
    let server = env!("CARGO_BIN_EXE_mcp-demo-docs");
    let config = json!({
        "transport": "stdio",
        "prefix": "docs",
        "command": "timeout",
        "args": ["1", server],
    });
    let allowed = NetworkPolicy {
        allow_private_network: false,
        allow_stdio: true,
    };
    let runtime = start(config, allowed).await;
    assert!(!runtime.is_closed());
    assert!(searched(&runtime).await);

    let closed = tokio::time::timeout(Duration::from_secs(10), async {
        while !runtime.is_closed() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    assert!(closed.await.is_ok(), "the session never said it closed");
    assert!(!searched(&runtime).await);
}
