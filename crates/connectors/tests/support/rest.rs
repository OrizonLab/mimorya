//! Starting a REST API connector as a run would (ADR-112), on the test API
//! (`mimorya_connectors::testing::serve_rest_http`).

use std::time::Duration;

use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::{ConnectorRuntime, InstanceContext, connector_type};
use mimorya_core::tools::ToolRef;
use serde_json::{Value, json};

/// A token the masking finds whole: letters and digits only.
pub const TOKEN: &str = "Zq7Wk2Lr9Xp4Nm6Vb3Hs";

/// An API at `url`, prefixed `crm`, with these operations.
pub fn api(url: &str, operations: Value) -> Value {
    json!({ "prefix": "crm", "url": url, "operations": operations })
}

/// An operation reading what the test API received.
pub fn echo(name: &str, method: &str, path: &str, parameters: Value) -> Value {
    json!({
        "name": name,
        "title": format!("Opération {name}"),
        "description": "Renvoie la requête reçue.",
        "method": method,
        "path": path,
        "parameters": parameters,
    })
}

/// Starts the connector as a run would, with its own call timeout.
pub async fn start_rest(
    config: Value,
    secret: Option<&str>,
    network: NetworkPolicy,
    call_timeout: Duration,
) -> anyhow::Result<ConnectorRuntime> {
    let context = InstanceContext {
        secret,
        network,
        call_timeout,
        ..InstanceContext::default()
    };
    connector_type("rest")
        .unwrap()
        .instantiate(&config, context)
        .await
}

/// The tool named `name`.
pub fn tool(runtime: &ConnectorRuntime, name: &str) -> ToolRef {
    runtime
        .tools
        .iter()
        .find(|tool| tool.info().name == name)
        .cloned()
        .unwrap_or_else(|| panic!("{name} among the tools"))
}
