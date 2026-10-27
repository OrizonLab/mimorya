//! The "Google Workspace" connector type (ADR-121): a company declares its
//! OAuth client in Google Cloud, and each of its people links their own
//! Workspace account there, granting delegated permissions only: their
//! Gmail and their calendar, never the domain's. Its tools, to come,
//! reach Google's APIs with the token of the person a run serves, as
//! Microsoft 365's reach Graph (ADR-117): what they read, the person could
//! read themselves.

use anyhow::bail;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::outbound::url_problem;
use crate::types::{Connector, ConnectorInfo, ConnectorRuntime, InstanceContext};

/// Its tools, named by the platform: a company's policies may name them.
/// None yet: a link is made before they come.
pub const GOOGLE_WORKSPACE_TOOLS: [&str; 0] = [];

/// A company's OAuth client, as stored: its client ID (checked by the
/// administration), the issuer its people sign in at (Google's), the Gmail
/// and Calendar APIs its tools call, and whether its tools may write. The
/// administration writes the addresses; tests point them at simulated
/// servers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoogleWorkspaceConfig {
    pub client_id: String,
    pub issuer: String,
    pub gmail: String,
    pub calendar: String,
    /// Sending mail and creating events in the person's name, each
    /// validated: its people are then asked those permissions too.
    #[serde(default)]
    pub write_tools: bool,
}

impl GoogleWorkspaceConfig {
    /// The stored configuration, or why it cannot be read (in French).
    pub fn read(config: &Value) -> anyhow::Result<Self> {
        serde_json::from_value(config.clone()).map_err(|_| {
            anyhow::anyhow!(
                "Déclaration Google Workspace illisible : supprimez le connecteur puis ajoutez-le de nouveau"
            )
        })
    }
}

pub struct GoogleWorkspaceConnector;

#[async_trait]
impl Connector for GoogleWorkspaceConnector {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            connector_type: mimorya_core::account_links::GOOGLE_WORKSPACE,
            name: "Google Workspace",
            description: "La boîte Gmail et l’agenda Google de chacun, que chaque personne relie elle-même à son compte Workspace : l’agent n’y lira que ce que la personne servie peut lire.",
            demo: false,
            permissions: &[],
        }
    }

    async fn instantiate(
        &self,
        config: &Value,
        context: InstanceContext<'_>,
    ) -> anyhow::Result<ConnectorRuntime> {
        let config = GoogleWorkspaceConfig::read(config)?;
        // As when it was saved: the operator may have narrowed the network
        // policy since.
        for address in [&config.issuer, &config.gmail, &config.calendar] {
            if let Some(problem) = url_problem(address, context.network) {
                bail!("{problem}");
            }
        }
        if context.secret.is_none() {
            bail!("Le secret de l’application n’est plus enregistré : saisissez-le de nouveau");
        }
        Ok(ConnectorRuntime {
            tools: Vec::new(),
            closer: None,
        })
    }
}
