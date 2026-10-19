//! Connector contract. A connector type is instantiated per tenant
//! (`connector_instances`) once per process, then reused across requests.

use std::time::Duration;

use async_trait::async_trait;
use mimorya_core::authz::PermissionInfo;
use mimorya_core::tools::ToolRef;
use serde_json::Value;

use crate::mcp::NetworkPolicy;

#[derive(Debug, Clone)]
pub struct ConnectorInfo {
    pub connector_type: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Demo connectors serve fictional data and are labelled as such in the UI.
    pub demo: bool,
    /// Business permissions introduced by this connector.
    pub permissions: &'static [PermissionInfo],
}

/// Releases what a live connector holds (processes, connections).
#[async_trait]
pub trait ConnectorCloser: Send + Sync {
    async fn close(&self);
}

/// A live connector instance: its tools and how to release its resources.
pub struct ConnectorRuntime {
    pub tools: Vec<ToolRef>,
    pub closer: Option<Box<dyn ConnectorCloser>>,
}

/// How long a connector may take to start (connect and list its tools).
pub const DEFAULT_START_TIMEOUT: Duration = Duration::from_secs(20);

/// What an instance is given besides its configuration.
#[derive(Clone, Copy)]
pub struct InstanceContext<'a> {
    /// The instance's secret (an access token), opened from the secret store.
    pub secret: Option<&'a str>,
    /// What the operator lets connectors reach.
    pub network: NetworkPolicy,
    pub start_timeout: Duration,
}

impl Default for InstanceContext<'_> {
    fn default() -> Self {
        Self {
            secret: None,
            network: NetworkPolicy::default(),
            start_timeout: DEFAULT_START_TIMEOUT,
        }
    }
}

#[async_trait]
pub trait Connector: Send + Sync {
    fn info(&self) -> ConnectorInfo;
    async fn instantiate(
        &self,
        config: &Value,
        context: InstanceContext<'_>,
    ) -> anyhow::Result<ConnectorRuntime>;
}
