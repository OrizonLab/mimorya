//! The tools a company's connectors give, and how an evaluation of a skill
//! version the agent proposed goes without those that reach outside the
//! platform (ADR-153): a proposal no administrator made active could have
//! the agent read what whom it runs for may read, then pass it to an MCP
//! server, a REST API or a linked account. A connector withheld is named as
//! unavailable, as one that could not start is (ADR-048), so that the agent
//! says what is missing.

use std::collections::{HashMap, HashSet};

use mimorya_agent::tools::ToolOverride;
use mimorya_connectors::connector_type;
use mimorya_core::tools::ToolRef;

use super::ConnectorIssue;

/// Why a connector that reaches outside is not offered.
const WITHHELD: &str = "Connecteur retenu pendant l’évaluation d’une proposition de l’agent";

#[derive(Default)]
pub struct TenantTools {
    pub tools: Vec<ToolRef>,
    pub overrides: HashMap<String, ToolOverride>,
    pub issues: Vec<ConnectorIssue>,
    /// The connectors given whose tools reach outside the platform.
    outside: Vec<Outside>,
}

/// A connector given whose tools reach outside the platform.
struct Outside {
    connector: String,
    connector_type: String,
    tools: Vec<String>,
}

impl TenantTools {
    /// Gives the tools of the connector `connector`, of type `type_name`,
    /// under their policies.
    pub(super) fn give(
        &mut self,
        connector: &str,
        type_name: &str,
        tools: &[ToolRef],
        overrides: HashMap<String, ToolOverride>,
    ) {
        if reaches_outside(type_name) {
            self.outside.push(Outside {
                connector: connector.to_owned(),
                connector_type: type_name.to_owned(),
                tools: tools.iter().map(|tool| tool.info().name.clone()).collect(),
            });
        }
        self.tools.extend(tools.iter().cloned());
        self.overrides.extend(overrides);
    }

    /// Without the tools of the connectors that reach outside the platform,
    /// each of them then named as unavailable.
    pub fn inside_only(mut self) -> Self {
        let withheld: HashSet<String> = self
            .outside
            .iter()
            .flat_map(|outside| outside.tools.iter().cloned())
            .collect();
        self.tools
            .retain(|tool| !withheld.contains(&tool.info().name));
        self.overrides.retain(|name, _| !withheld.contains(name));
        for outside in std::mem::take(&mut self.outside) {
            self.issues.push(ConnectorIssue {
                connector: outside.connector,
                connector_type: outside.connector_type,
                message: WITHHELD.into(),
            });
        }
        self
    }
}

/// Whether the tools of `type_name` reach outside the platform; a type
/// unknown here is taken to.
fn reaches_outside(type_name: &str) -> bool {
    connector_type(type_name).is_none_or(|definition| definition.info().reaches_outside)
}

#[cfg(test)]
mod tests {
    use mimorya_connectors::{InstanceContext, connector_types};
    use serde_json::json;

    use super::super::reach::added_by_companies;
    use super::*;

    fn names(tools: &[ToolRef]) -> Vec<String> {
        tools.iter().map(|tool| tool.info().name.clone()).collect()
    }

    #[tokio::test]
    async fn inside_only_withholds_the_tools_of_the_connectors_reaching_outside() {
        let demo = connector_type("demo_interventions")
            .unwrap()
            .instantiate(&json!({}), InstanceContext::default())
            .await
            .unwrap();
        let (kept, sent) = demo.tools.split_at(1);
        let policy = |tools: &[ToolRef]| {
            HashMap::from([(tools[0].info().name.clone(), ToolOverride::default())])
        };
        let mut given = TenantTools::default();
        given.give("Interventions", "demo_interventions", kept, policy(kept));
        // The same tools, as an MCP server's would be.
        given.give("Procédures", "mcp", sent, policy(sent));
        assert_eq!(given.tools.len(), demo.tools.len());

        let inside = given.inside_only();
        assert_eq!(names(&inside.tools), names(kept));
        assert_eq!(inside.overrides, policy(kept));
        assert_eq!(
            inside.issues,
            [ConnectorIssue {
                connector: "Procédures".into(),
                connector_type: "mcp".into(),
                message: WITHHELD.into(),
            }]
        );
    }

    #[test]
    fn only_the_demo_connector_stays_inside() {
        let inside: Vec<&str> = connector_types()
            .iter()
            .map(|definition| definition.info())
            .filter(|info| !info.reaches_outside)
            .map(|info| info.connector_type)
            .collect();
        assert_eq!(inside, ["demo_interventions"]);
        for type_name in ["mcp", "rest", "microsoft365", "google_workspace", "inconnu"] {
            assert!(reaches_outside(type_name), "{type_name}");
        }
    }

    /// What a company adds itself (its own server, API or accounts) is
    /// outside the platform: a new type of the kind is withheld too.
    #[test]
    fn every_type_a_company_adds_reaches_outside() {
        for definition in connector_types() {
            let info = definition.info();
            if added_by_companies(info.connector_type) {
                assert!(info.reaches_outside, "{}", info.connector_type);
            }
        }
    }
}
