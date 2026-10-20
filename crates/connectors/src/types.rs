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

/// Releases what a live connector holds (processes, connections), and tells
/// whether its connection is gone for good (a local server that exited): an
/// instance kept between runs is then started again rather than served.
#[async_trait]
pub trait ConnectorCloser: Send + Sync {
    async fn close(&self);

    /// Whether the connection is closed and cannot recover by itself.
    fn is_closed(&self) -> bool {
        false
    }
}

/// A live connector instance: its tools and how to release its resources.
pub struct ConnectorRuntime {
    pub tools: Vec<ToolRef>,
    pub closer: Option<Box<dyn ConnectorCloser>>,
}

impl ConnectorRuntime {
    /// Whether its connection is gone for good: no call can succeed again.
    pub fn is_closed(&self) -> bool {
        self.closer
            .as_ref()
            .is_some_and(|closer| closer.is_closed())
    }
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
    /// Bounded in time (`InstanceContext::start_timeout` for what connects
    /// to a server): every run that needs the instance waits for its start.
    async fn instantiate(
        &self,
        config: &Value,
        context: InstanceContext<'_>,
    ) -> anyhow::Result<ConnectorRuntime>;
}
