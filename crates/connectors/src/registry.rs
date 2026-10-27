//! Connector types available on this platform build.

use std::sync::Arc;

use crate::demo_interventions::DemoInterventionsConnector;
use crate::google_workspace::GoogleWorkspaceConnector;
use crate::mcp::McpConnector;
use crate::microsoft365::Microsoft365Connector;
use crate::rest::RestConnector;
use crate::types::Connector;

pub fn connector_types() -> Vec<Arc<dyn Connector>> {
    vec![
        Arc::new(DemoInterventionsConnector),
        Arc::new(McpConnector),
        Arc::new(RestConnector),
        Arc::new(Microsoft365Connector),
        Arc::new(GoogleWorkspaceConnector),
    ]
}

pub fn connector_type(name: &str) -> Option<Arc<dyn Connector>> {
    connector_types()
        .into_iter()
        .find(|c| c.info().connector_type == name)
}
