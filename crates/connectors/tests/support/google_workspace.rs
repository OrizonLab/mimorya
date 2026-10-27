//! The Google Workspace tools as a run calls them (ADR-122): the connector
//! started on the test Gmail and Calendar, its tools reading or writing too
//! (ADR-123); run as `linked_accounts` runs them.

use std::time::Duration;

use mimorya_connectors::testing::FakeGoogle;
use mimorya_connectors::{InstanceContext, connector_type};
use mimorya_core::account_links::GOOGLE_WORKSPACE;
use mimorya_core::tools::ToolRef;
use serde_json::{Value, json};

use super::linked_accounts::LOCAL;

fn client(google: &FakeGoogle) -> Value {
    json!({
        "clientId": "123456789012-mimorya.apps.googleusercontent.com",
        "issuer": "http://127.0.0.1:9",
        "gmail": google.gmail,
        "calendar": google.calendar,
    })
}

/// The connector's tools, on the test Google, started as a run would.
pub async fn google_tools(google: &FakeGoogle) -> Vec<ToolRef> {
    started(&client(google)).await
}

/// Its tools when the company lets them write (ADR-123).
pub async fn google_writing_tools(google: &FakeGoogle) -> Vec<ToolRef> {
    let mut config = client(google);
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
    connector_type(GOOGLE_WORKSPACE)
        .unwrap()
        .instantiate(config, context)
        .await
        .unwrap()
        .tools
}
