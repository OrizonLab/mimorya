//! The "Microsoft 365" connector type (ADR-116): a company declares its
//! application in its Microsoft Entra directory, and each of its people
//! links their own mailbox and calendar there, granting delegated
//! permissions only. Its tools (ADR-117) reach Microsoft Graph with the
//! token of the person a run serves, never with the application's own
//! rights: what they read, the person could read themselves. Those that
//! write (ADR-119), offered when the company lets them, send an e-mail or
//! create an event in the person's name, as validated.

mod calendar_create;
mod calendar_events;
mod mail_read;
mod mail_search;
mod mail_send;
mod shape;

use anyhow::bail;
use async_trait::async_trait;
use mimorya_core::account_links::LinkKind;
use mimorya_core::tools::typed_tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use self::calendar_create::CalendarCreate;
use self::calendar_events::CalendarEvents;
use self::mail_read::MailRead;
use self::mail_search::MailSearch;
use self::mail_send::MailSend;
use crate::delegated::ProviderApi;
use crate::outbound::{http_client, url_problem};
use crate::types::{Connector, ConnectorInfo, ConnectorRuntime, InstanceContext};

/// Its tools, named by the platform: a company's policies may name them.
pub const MICROSOFT365_TOOLS: [&str; 5] = [
    "m365_mail_search",
    "m365_mail_read",
    "m365_calendar_events",
    "m365_mail_send",
    "m365_calendar_event_create",
];
/// Those that write, offered when the company lets them (ADR-119).
pub const MICROSOFT365_WRITE_TOOLS: [&str; 2] = ["m365_mail_send", "m365_calendar_event_create"];

/// A company's application, as stored: its directory and client IDs (the
/// GUIDs the administration checked), the issuer its people sign in at (the
/// directory's), the Graph its tools call, and whether its tools may write
/// (ADR-119). The administration writes the addresses; tests point them at
/// simulated servers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Microsoft365Config {
    pub directory: Uuid,
    pub client_id: String,
    pub issuer: String,
    pub graph: String,
    /// Sending mail and creating events in the person's name, each
    /// validated: its people are then asked those permissions too. Absent
    /// from a declaration stored before, which reads only.
    #[serde(default)]
    pub write_tools: bool,
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
            connector_type: LinkKind::Microsoft365.connector_type(),
            name: "Microsoft 365",
            description: "La boîte e-mail et l’agenda Microsoft 365 de chacun, que chaque personne relie elle-même à son compte : l’agent n’y lit que ce que la personne servie peut lire, et n’y écrit, si l’entreprise le permet, que ce qui est validé.",
            demo: false,
            permissions: &[],
            reaches_outside: true,
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
        let graph = ProviderApi::new(
            LinkKind::Microsoft365,
            &config.graph,
            http_client(context.network)?,
            context.call_timeout,
        )?;
        let mut tools = vec![
            typed_tool(MailSearch {
                graph: graph.clone(),
            }),
            typed_tool(MailRead {
                graph: graph.clone(),
            }),
            typed_tool(CalendarEvents {
                graph: graph.clone(),
            }),
        ];
        if config.write_tools {
            tools.push(typed_tool(MailSend {
                graph: graph.clone(),
            }));
            tools.push(typed_tool(CalendarCreate { graph }));
        }
        Ok(ConnectorRuntime {
            tools,
            closer: None,
        })
    }
}
