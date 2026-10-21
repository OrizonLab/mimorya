//! The tool policies of an instance, as runs apply them: read strictly (a
//! policy that cannot be read makes the connector unavailable, since
//! ignoring it could drop a restriction an administrator set), scoped to the
//! instance's own tools; an MCP tool always requires `connector:use`, and a
//! description an administrator wrote keeps naming the server (ADR-057).

use std::collections::HashMap;

use mimorya_agent::tools::ToolOverride;
use mimorya_connectors::mcp::{CONNECTOR_USE, mcp_tool_description};
use mimorya_db::repositories::connectors::ConnectorInstanceRow;

pub type Policies = HashMap<String, ToolOverride>;

/// The stored policies, or why they cannot be applied.
pub fn read_policies(instance: &ConnectorInstanceRow) -> Result<Policies, String> {
    serde_json::from_value(instance.tool_policies.clone()).map_err(|error| {
        format!("Politiques d’outils illisibles : enregistrez-les de nouveau (détail : {error})")
    })
}

/// The policies of these tools, as runs apply them.
pub fn scoped_overrides(
    instance: &ConnectorInstanceRow,
    policies: Policies,
    tools: &[String],
) -> Policies {
    policies
        .into_iter()
        .filter(|(name, _)| tools.contains(name))
        .map(|(name, mut policy)| {
            if instance.connector_type == "mcp" {
                if let Some(permissions) = policy.permissions.as_mut()
                    && !permissions.iter().any(|p| p == CONNECTOR_USE)
                {
                    permissions.push(CONNECTOR_USE.into());
                }
                let prefix = instance.config["prefix"].as_str().unwrap_or_default();
                policy.description = policy
                    .description
                    .map(|written| mcp_tool_description(prefix, &written));
            }
            (name, policy)
        })
        .collect()
}
