//! Connectors (`connector:manage`, ADR-025): the company's instances with
//! their state, what may be added, and the test of a server. Tokens are
//! never returned, only whether one is stored.

use mimorya_connectors::ConnectorInfo;
use mimorya_core::autonomy::ToolRisk as CoreRisk;
use mimorya_db::repositories::connectors::ConnectorInstanceRow;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::contract::{ToolRisk, stored_text};
use crate::http::iso;
use crate::services::admin::{ConnectorTestOutcome, TestedTool as Tested};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStatus {
    /// No start tried since it was added or changed.
    Untested,
    Ok,
    Error,
    Disabled,
}

/// An administrator's policy for one tool, as stored. Every field is
/// optional: absent or `null`, the tool's own declaration applies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolPolicy {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub enabled: Option<bool>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub permissions: Option<Vec<String>>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub risk: Option<ToolRisk>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub requires_approval: Option<bool>,
}

/// An MCP server reached over HTTPS (editable from the administration).
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct McpEndpoint {
    /// Its tools are named `mcp_<prefix>_<tool>`.
    pub prefix: String,
    pub url: String,
    pub token_stored: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorView {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub connector_type: String,
    pub name: String,
    pub enabled: bool,
    pub status: ConnectorStatus,
    pub last_error: Option<String>,
    /// As stored: a policy runs cannot read makes the connector unavailable,
    /// with `lastError` saying so, rather than hiding it here.
    #[ts(type = "Record<string, ToolPolicy>")]
    pub tool_policies: Value,
    pub mcp: Option<McpEndpoint>,
    pub updated_at: String,
}

impl ConnectorView {
    pub fn from_row(i: &ConnectorInstanceRow, mcp: Option<McpEndpoint>) -> anyhow::Result<Self> {
        Ok(Self {
            id: i.id,
            connector_type: i.connector_type.clone(),
            name: i.name.clone(),
            enabled: i.enabled,
            status: stored_text("connector status", &i.status)?,
            last_error: i.last_error.clone(),
            tool_policies: i.tool_policies.clone(),
            mcp,
            updated_at: iso(&i.updated_at),
        })
    }
}

/// A kind of connector the platform provides.
#[derive(Debug, Serialize, TS)]
pub struct ConnectorType {
    #[serde(rename = "type")]
    pub connector_type: String,
    pub name: String,
    pub description: String,
    /// Serves fictitious data, labelled as such.
    pub demo: bool,
}

impl From<&ConnectorInfo> for ConnectorType {
    fn from(info: &ConnectorInfo) -> Self {
        Self {
            connector_type: info.connector_type.into(),
            name: info.name.into(),
            description: info.description.into(),
            demo: info.demo,
        }
    }
}

/// A permission a tool may require: one an agent can hold.
#[derive(Debug, Serialize, TS)]
pub struct DelegablePermission {
    pub permission: String,
    pub label: String,
}

/// `GET /admin/connectors`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Connectors {
    pub instances: Vec<ConnectorView>,
    pub types: Vec<ConnectorType>,
    pub permissions: Vec<DelegablePermission>,
    pub risks: Vec<ToolRisk>,
    /// The server can seal tokens (`MIMORYA_MASTER_KEY`).
    pub secrets_available: bool,
    /// The operator lets connectors reach its private network (private deployment).
    pub allow_private_network: bool,
}

/// A tool a server declared, with the policy in force.
#[derive(Debug, Serialize, TS)]
pub struct TestedTool {
    pub name: String,
    pub title: String,
    pub description: String,
    pub enabled: bool,
    pub risk: ToolRisk,
    pub permissions: Vec<String>,
}

impl From<&Tested> for TestedTool {
    fn from(t: &Tested) -> Self {
        Self {
            name: t.name.clone(),
            title: t.title.clone(),
            description: t.description.clone(),
            enabled: t.enabled,
            risk: t.risk.into(),
            permissions: t.permissions.clone(),
        }
    }
}

/// `POST /admin/connectors/{id}/test`.
#[derive(Debug, Serialize, TS)]
pub struct ConnectorTest {
    pub ok: bool,
    pub tools: Vec<TestedTool>,
    /// Why it could not start (without the token).
    pub error: Option<String>,
}

impl From<&ConnectorTestOutcome> for ConnectorTest {
    fn from(outcome: &ConnectorTestOutcome) -> Self {
        Self {
            ok: outcome.error.is_none(),
            tools: outcome.tools.iter().map(Into::into).collect(),
            error: outcome.error.clone(),
        }
    }
}

/// Every risk class, from harmless to irreversible.
pub fn risks() -> Vec<ToolRisk> {
    CoreRisk::ALL.into_iter().map(Into::into).collect()
}
