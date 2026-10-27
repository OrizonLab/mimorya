//! The Microsoft 365 tools as a run calls them (ADR-117): the connector
//! started on the test Graph, its tools reading or writing too (ADR-119);
//! run as `linked_accounts` runs them.

use std::time::Duration;

use mimorya_connectors::{InstanceContext, connector_type};
use mimorya_core::account_links::MICROSOFT365;
use mimorya_core::tools::ToolRef;
use serde_json::{Value, json};

use super::linked_accounts::LOCAL;

/// The company's application, its Graph the test one.
pub fn application(graph: &str) -> Value {
    json!({
        "directory": "72f988bf-86f1-41af-91ab-2d7cd011db47",
        "clientId": "0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0",
        "issuer": "http://127.0.0.1:9/v2.0",
        "graph": graph,
    })
}

/// The connector's tools, started as a run would.
pub async fn microsoft365_tools(graph: &str) -> Vec<ToolRef> {
    started(&application(graph)).await
}

/// Its tools when the company lets them write (ADR-119).
pub async fn microsoft365_writing_tools(graph: &str) -> Vec<ToolRef> {
    let mut config = application(graph);
    config["writeTools"] = json!(true);
    started(&config).await
}

async fn started(config: &Value) -> Vec<ToolRef> {
    let context = InstanceContext {
        secret: Some("secret-de-l-application"),
        network: LOCAL,
        call_timeout: Duration::from_secs(5),
        ..InstanceContext::default()
    };
    connector_type(MICROSOFT365)
        .unwrap()
        .instantiate(config, context)
        .await
        .unwrap()
        .tools
}
