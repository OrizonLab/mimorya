//! Starting an MCP connector as a run would, and calling one of its tools as
//! Lucas once an administrator mapped it to a read of interventions.

use std::collections::HashMap;
use std::time::Duration;

use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::{ToolCallEvaluation, ToolOverride, ToolRegistry};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::{ConnectorRuntime, InstanceContext, connector_type};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use mimorya_core::tools::ToolResult;
use serde_json::{Value, json};

use super::executor_for;

/// A private deployment: its own network is reachable.
pub const PRIVATE: NetworkPolicy = NetworkPolicy {
    allow_private_network: true,
    allow_stdio: false,
};

/// A server reached over HTTP, its tools prefixed `docs`.
pub fn http(url: &str) -> Value {
    json!({ "transport": "http", "prefix": "docs", "url": url })
}

/// The port of a local test server (`http://127.0.0.1:<port>/mcp`).
pub fn port_of(url: &str) -> u16 {
    url.trim_start_matches("http://127.0.0.1:")
        .split('/')
        .next()
        .and_then(|port| port.parse().ok())
        .expect("a local test server's address")
}

/// Starts an MCP instance as a run would.
pub async fn start_mcp(
    config: Value,
    secret: Option<&str>,
    network: NetworkPolicy,
) -> anyhow::Result<ConnectorRuntime> {
    let start_timeout = InstanceContext::default().start_timeout;
    start_mcp_within(config, secret, network, start_timeout).await
}

pub async fn start_mcp_within(
    config: Value,
    secret: Option<&str>,
    network: NetworkPolicy,
    start_timeout: Duration,
) -> anyhow::Result<ConnectorRuntime> {
    let context = InstanceContext {
        secret,
        network,
        start_timeout,
        ..InstanceContext::default()
    };
    connector_type("mcp")
        .unwrap()
        .instantiate(&config, context)
        .await
}

/// Runs the tool `name` as Lucas, the tool mapped to a read of interventions.
pub async fn call_as_reader(runtime: &ConnectorRuntime, name: &str, input: Value) -> ToolResult {
    let read = ToolOverride {
        risk: Some(ToolRisk::Read),
        permissions: Some(vec!["intervention:read".into()]),
        ..Default::default()
    };
    let overrides = HashMap::from([(name.to_string(), read)]);
    let registry = ToolRegistry::new(runtime.tools.clone(), &overrides).unwrap();
    let ports = InMemoryPorts::new(user::LUCAS, uuid::Uuid::new_v4());
    let lucas = executor_for(registry, user::LUCAS, &ports);
    let call = LlmToolCall {
        id: "1".into(),
        name: name.into(),
        input,
    };
    let ToolCallEvaluation::Allow { tool, input } = lucas.evaluate(&call) else {
        panic!("expected {name} to run directly");
    };
    lucas.execute(&tool, input).await.0
}
