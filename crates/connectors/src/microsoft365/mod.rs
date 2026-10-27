//! The "Microsoft 365" connector type (ADR-116): a company declares its
//! application in its Microsoft Entra directory, and each of its people
//! links their own mailbox and calendar there, granting delegated
//! permissions only. Its tools (ADR-117) reach Microsoft Graph with the
//! token of the person a run serves, never with the application's own
//! rights: what they read, the person could read themselves.

mod calendar_events;
mod graph;
mod mail_read;
mod mail_search;
mod reach;
mod shape;

use anyhow::bail;
use async_trait::async_trait;
use mimorya_core::tools::typed_tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use self::calendar_events::CalendarEvents;
use self::graph::Graph;
use self::mail_read::MailRead;
use self::mail_search::MailSearch;
use crate::outbound::{http_client, url_problem};
use crate::types::{Connector, ConnectorInfo, ConnectorRuntime, InstanceContext};

/// Its tools, named by the platform: a company's policies may name them.
pub const MICROSOFT365_TOOLS: [&str; 3] =
    ["m365_mail_search", "m365_mail_read", "m365_calendar_events"];

/// A company's application, as stored: its directory and client IDs (the
/// GUIDs the administration checked), the issuer its people sign in at (the
/// directory's), and the Graph its tools call. The administration writes
/// the last two; tests point them at simulated servers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Microsoft365Config {
    pub directory: Uuid,
    pub client_id: String,
    pub issuer: String,
    pub graph: String,
}

impl Microsoft365Config {
    /// The stored configuration, or why it cannot be read (in French).
    pub fn read(config: &Value) -> anyhow::Result<Self> {
        serde_json::from_value(config.clone()).map_err(|_| {
            anyhow::anyhow!(
                "Déclaration Microsoft 365 illisible : supprimez le connecteur puis ajoutez-le de nouveau"
            )
        })
    }
}

pub struct Microsoft365Connector;

#[async_trait]
impl Connector for Microsoft365Connector {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            connector_type: mimorya_core::account_links::MICROSOFT365,
            name: "Microsoft 365",
            description: "La boîte e-mail et l’agenda Microsoft 365 de chacun, que chaque personne relie elle-même à son compte : l’agent n’y lit que ce que la personne servie peut lire.",
            demo: false,
            permissions: &[],
        }
    }

    async fn instantiate(
        &self,
        config: &Value,
        context: InstanceContext<'_>,
    ) -> anyhow::Result<ConnectorRuntime> {
        let config = Microsoft365Config::read(config)?;
        // As when it was saved: the operator may have narrowed the network
        // policy since.
        for address in [&config.issuer, &config.graph] {
            if let Some(problem) = url_problem(address, context.network) {
                bail!("{problem}");
            }
        }
        if context.secret.is_none() {
            bail!("Le secret de l’application n’est plus enregistré : saisissez-le de nouveau");
        }
        let graph = Graph::new(
            &config.graph,
            http_client(context.network)?,
            context.call_timeout,
        )?;
        Ok(ConnectorRuntime {
            tools: vec![
                typed_tool(MailSearch {
                    graph: graph.clone(),
                }),
                typed_tool(MailRead {
                    graph: graph.clone(),
                }),
                typed_tool(CalendarEvents { graph }),
            ],
            closer: None,
        })
    }
}
