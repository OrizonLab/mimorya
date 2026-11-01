//! Demo connector "Interventions" — FICTIONAL data, in-memory state. It shows
//! how a connector enforces the caller's data scope: every record is checked
//! with `ctx.authz` before being returned, and the existence of inaccessible
//! records is never revealed.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use mimorya_core::authz::{Enforcement, PermissionInfo};
use mimorya_core::tools::{ToolRef, typed_tool};
use serde_json::Value;

use super::store::DemoInterventionsStore;
use super::tools::{
    InterventionsGet, InterventionsSearch, PlanningApply, PlanningPropose, TeamAvailability,
};
use crate::types::{Connector, ConnectorInfo, ConnectorRuntime, InstanceContext};

pub fn demo_intervention_tools(store: Arc<DemoInterventionsStore>) -> Vec<ToolRef> {
    vec![
        typed_tool(InterventionsSearch {
            store: store.clone(),
        }),
        typed_tool(InterventionsGet {
            store: store.clone(),
        }),
        typed_tool(TeamAvailability {
            store: store.clone(),
        }),
        typed_tool(PlanningPropose {
            store: store.clone(),
        }),
        typed_tool(PlanningApply { store }),
    ]
}

const PERMISSIONS: &[PermissionInfo] = &[
    PermissionInfo {
        permission: "intervention:read",
        label: "Consulter les interventions",
        delegable: true,
        // Each intervention is checked (`access.rs`).
        enforcement: Enforcement::PerResource,
    },
    PermissionInfo {
        permission: "intervention:write",
        label: "Modifier les interventions",
        delegable: true,
        // No tool writes interventions yet.
        enforcement: Enforcement::NotYet,
    },
    PermissionInfo {
        permission: "planning:read",
        label: "Consulter les plannings",
        delegable: true,
        enforcement: Enforcement::Holder,
    },
    PermissionInfo {
        permission: "planning:propose",
        label: "Proposer des plannings",
        delegable: true,
        enforcement: Enforcement::Holder,
    },
    PermissionInfo {
        permission: "planning:apply",
        label: "Appliquer des plannings",
        delegable: true,
        enforcement: Enforcement::Holder,
    },
];

pub struct DemoInterventionsConnector;

#[async_trait]
impl Connector for DemoInterventionsConnector {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            connector_type: "demo_interventions",
            name: "Interventions (démonstration)",
            description: "Connecteur de démonstration : comptes rendus d’intervention et planning d’une PME fictive. Données inventées, état en mémoire.",
            demo: true,
            permissions: PERMISSIONS,
            reaches_outside: false,
        }
    }

    /// `config.referenceDate` (AAAA-MM-JJ) pins the dataset's week; today otherwise.
    async fn instantiate(
        &self,
        config: &Value,
        _context: InstanceContext<'_>,
    ) -> anyhow::Result<ConnectorRuntime> {
        let reference = config["referenceDate"]
            .as_str()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .unwrap_or_else(|| Utc::now().date_naive());
        let store = Arc::new(DemoInterventionsStore::new(reference));
        Ok(ConnectorRuntime {
            tools: demo_intervention_tools(store),
            closer: None,
        })
    }
}
