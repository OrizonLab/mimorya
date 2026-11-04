//! An MCP server's sessions as the tests see them (ADR-171): the
//! administration's test of a connector, whether runs are given its tools
//! here or in another process that hears none of this one's notices, and
//! an authorization changed in the database as an operator could.

use axum::http::StatusCode;
use mimorya_api::secrets::SecretStore;
use mimorya_api::services::connectors::{ConnectorService, ConnectorSettings};
use mimorya_api::services::invalidations::Invalidations;
use mimorya_core::demo::tenant;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, TestApi};

/// The administration's test of the connector: its tools, or why not.
pub async fn tested(admin: &Client<'_>, id: &str) -> Value {
    let tested = admin
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await;
    assert_eq!(tested.status, StatusCode::OK, "{}", tested.text());
    tested.json()
}

pub fn said(outcome: &Value) -> String {
    outcome["error"].as_str().unwrap_or_default().to_string()
}

/// Whether runs are given the server's tools (prefixed with `prefix`).
pub async fn runs_see(api: &TestApi, prefix: &str) -> bool {
    sees(&api.app.services.connectors, prefix).await
}

/// Whether the runs of the process holding `connectors` are given them.
pub async fn sees(connectors: &ConnectorService, prefix: &str) -> bool {
    let tools = connectors.tools_for(tenant::ID).await.unwrap();
    let namespace = format!("mcp_{prefix}_");
    tools
        .tools
        .iter()
        .any(|tool| tool.info().name.starts_with(&namespace))
}

/// Another process's connectors, which hear none of this one's notices.
pub fn another_process(api: &TestApi) -> ConnectorService {
    let services = &api.app.services;
    let config = &services.config;
    let settings = ConnectorSettings {
        network: config.mcp_network,
        start_timeout: config.mcp_start_timeout,
    };
    let secrets = SecretStore::new(config.secrets.clone());
    ConnectorService::new(
        services.db.clone(),
        secrets,
        settings,
        Invalidations::new().0,
    )
}

/// Changes the stored authorization as an operator could, then releases the
/// live instance.
pub async fn alter(api: &TestApi, id: &str, sql: &str) {
    let id: Uuid = id.parse().unwrap();
    let services = &api.app.services;
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(sql).bind(id).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    services.connectors.invalidate(id).await;
}

/// What moves the connector's declared token endpoint to `to`, for `alter`.
pub fn token_endpoint(to: &str) -> String {
    format!(
        "update connector_instances set config = jsonb_set(config, \
         '{{oauth,tokenEndpoint}}', '\"{to}\"') where id = $1"
    )
}
