//! The tool policies of an instance, as runs apply them: read strictly (a
//! policy that cannot be read makes the connector unavailable, since
//! ignoring it could drop a restriction an administrator set), scoped to the
//! instance's own tools; an MCP, REST, Microsoft 365 or Google Workspace
//! tool always requires `connector:use` (ADR-025, ADR-112, ADR-117,
//! ADR-121), and a description an administrator
//! wrote keeps naming the server (ADR-057), an MCP server's tools being the
//! only ones whose description may be replaced.

use std::collections::HashMap;

use mimorya_agent::tools::ToolOverride;
use mimorya_connectors::mcp::mcp_tool_description;
use mimorya_connectors::outbound::CONNECTOR_USE;
use mimorya_core::account_links::LinkKind;
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
            let type_name = instance.connector_type.as_str();
            let added = matches!(type_name, "mcp" | "rest") || LinkKind::of(type_name).is_some();
            if added
                && let Some(permissions) = policy.permissions.as_mut()
                && !permissions.iter().any(|p| p == CONNECTOR_USE)
            {
                permissions.push(CONNECTOR_USE.into());
            }
            if instance.connector_type == "mcp" {
                let prefix = instance.config["prefix"].as_str().unwrap_or_default();
                policy.description = policy
                    .description
                    .map(|written| mcp_tool_description(prefix, &written));
            } else {
                // Only an MCP server's tools have theirs replaced (ADR-057).
                policy.description = None;
            }
            (name, policy)
        })
        .collect()
}
