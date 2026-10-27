//! Which unavailable connectors a run is told about (ADR-048): those whose
//! tools its user could reach. While a connector is down its tools are
//! unknown: reach is judged on what every tool of its type requires,
//! `connector:use` for an MCP, REST, Microsoft 365 or Google Workspace tool
//! (ADR-116, ADR-121), one of
//! the business permissions it introduces for another type's tools. It is an approximation, broader than
//! the tools a run would be offered (a tool may require several permissions,
//! and the instance's policies may withhold it): a user may be told of a
//! connector none of whose tools they would be offered.

use mimorya_connectors::connector_type;
use mimorya_connectors::outbound::CONNECTOR_USE;
use mimorya_core::account_links::LinkKind;

use super::ConnectorIssue;

/// The names of the unavailable connectors whose tools a holder of these
/// permissions could reach, in the order of the issues.
pub fn unavailable_in_reach(
    issues: &[ConnectorIssue],
    holds: impl Fn(&str) -> bool,
) -> Vec<String> {
    issues
        .iter()
        .filter(|issue| in_reach(&issue.connector_type, &holds))
        .map(|issue| issue.connector.clone())
        .collect()
}

fn in_reach(type_name: &str, holds: &impl Fn(&str) -> bool) -> bool {
    if added_by_companies(type_name) {
        return holds(CONNECTOR_USE);
    }
    connector_type(type_name).is_some_and(|definition| {
        definition
            .info()
            .permissions
            .iter()
            .any(|p| holds(p.permission))
    })
}

/// The types a company adds itself, whose tools require `connector:use`.
fn added_by_companies(type_name: &str) -> bool {
    matches!(type_name, "mcp" | "rest") || LinkKind::of(type_name).is_some()
}

#[cfg(test)]
mod tests {
    use mimorya_connectors::{InstanceContext, connector_types};
    use serde_json::json;

    use super::*;

    fn issue(connector_type: &str, name: &str) -> ConnectorIssue {
        ConnectorIssue {
            connector: name.into(),
            connector_type: connector_type.into(),
            message: "Connecteur indisponible".into(),
        }
    }

    #[test]
    fn names_the_connectors_whose_tools_the_user_could_reach() {
        let issues = [
            issue("mcp", "Documentation"),
            issue("demo_interventions", "Interventions"),
            issue("rest", "Facturation"),
            issue("microsoft365", "Messagerie"),
            issue("google_workspace", "Gmail"),
            issue("inconnu", "Ancien type"),
        ];
        let member = |p: &str| p == CONNECTOR_USE;
        assert_eq!(
            unavailable_in_reach(&issues, member),
            ["Documentation", "Facturation", "Messagerie", "Gmail"]
        );
        let planner = |p: &str| p == "planning:read";
        assert_eq!(unavailable_in_reach(&issues, planner), ["Interventions"]);
        // Broader than the tools offered: no tool requires this one alone.
        let writer = |p: &str| p == "intervention:write";
        assert_eq!(unavailable_in_reach(&issues, writer), ["Interventions"]);
        assert!(unavailable_in_reach(&issues, |_| false).is_empty());
        let both = |p: &str| p == CONNECTOR_USE || p == "intervention:read";
        assert_eq!(
            unavailable_in_reach(&issues, both),
            [
                "Documentation",
                "Interventions",
                "Facturation",
                "Messagerie",
                "Gmail"
            ]
        );
    }

    /// The rule reach relies on, for every type but those a company adds
    /// (MCP, REST, Microsoft 365, Google Workspace): each of its tools
    /// requires one of the permissions it introduces.
    #[tokio::test]
    async fn every_tool_of_a_type_requires_a_permission_it_introduces() {
        for definition in connector_types() {
            let info = definition.info();
            if added_by_companies(info.connector_type) {
                continue;
            }
            let runtime = definition
                .instantiate(&json!({}), InstanceContext::default())
                .await
                .expect("instantiated without configuration");
            assert!(!runtime.tools.is_empty(), "{}", info.connector_type);
            for tool in &runtime.tools {
                let tool = tool.info();
                assert!(
                    tool.required_permissions
                        .iter()
                        .any(|p| info.permissions.iter().any(|i| i.permission == p)),
                    "{} : {}",
                    info.connector_type,
                    tool.name
                );
            }
        }
    }
}
