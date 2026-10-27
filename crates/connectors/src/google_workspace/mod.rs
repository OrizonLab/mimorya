//! The "Google Workspace" connector type (ADR-121): a company declares its
//! OAuth client in Google Cloud, and each of its people links their own
//! Workspace account there, granting delegated permissions only: their
//! Gmail and their calendar, never the domain's. Its tools (ADR-122) reach
//! Gmail and Google Calendar with the token of the person a run serves, as
//! Microsoft 365's reach Graph (`delegated`): what they read, the person
//! could read themselves. Those that write (ADR-123), offered when the
//! company lets them, send an e-mail or create an event in the person's
//! name, as validated.

mod calendar_create;
mod calendar_events;
mod gmail_read;
mod gmail_search;
mod gmail_send;
mod message;
mod mime;
mod outgoing;
mod text;

use anyhow::bail;
use async_trait::async_trait;
use mimorya_core::account_links::LinkKind;
use mimorya_core::tools::typed_tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use self::calendar_create::CalendarCreate;
use self::calendar_events::CalendarEvents;
use self::gmail_read::GmailRead;
use self::gmail_search::GmailSearch;
use self::gmail_send::GmailSend;
use crate::delegated::ProviderApi;
use crate::outbound::{http_client, url_problem};
use crate::types::{Connector, ConnectorInfo, ConnectorRuntime, InstanceContext};

/// Its tools, named by the platform: a company's policies may name them.
pub const GOOGLE_WORKSPACE_TOOLS: [&str; 5] = [
    "google_gmail_search",
    "google_gmail_read",
    "google_calendar_events",
    "google_gmail_send",
    "google_calendar_event_create",
];
/// Those that write, offered when the company lets them (ADR-123).
pub const GOOGLE_WORKSPACE_WRITE_TOOLS: [&str; 2] =
    ["google_gmail_send", "google_calendar_event_create"];

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
            connector_type: LinkKind::GoogleWorkspace.connector_type(),
            name: "Google Workspace",
            description: "La boîte Gmail et l’agenda Google de chacun, que chaque personne relie elle-même à son compte Workspace : l’agent n’y lit que ce que la personne servie peut lire, et n’y écrit, si l’entreprise le permet, que ce qui est validé.",
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
        let kind = LinkKind::GoogleWorkspace;
        let client = http_client(context.network)?;
        let gmail = ProviderApi::new(kind, &config.gmail, client.clone(), context.call_timeout)?;
        let calendar = ProviderApi::new(kind, &config.calendar, client, context.call_timeout)?;
        let mut tools = vec![
            typed_tool(GmailSearch {
                gmail: gmail.clone(),
            }),
            typed_tool(GmailRead {
                gmail: gmail.clone(),
            }),
            typed_tool(CalendarEvents {
                calendar: calendar.clone(),
            }),
        ];
        if config.write_tools {
            tools.push(typed_tool(GmailSend { gmail }));
            tools.push(typed_tool(CalendarCreate { calendar }));
        }
        Ok(ConnectorRuntime {
            tools,
            closer: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use mimorya_core::account_links::{GOOGLE_CALENDAR, GOOGLE_GMAIL};
    use serde_json::json;

    use super::*;

    fn declared(issuer: &str) -> Value {
        json!({
            "clientId": "123456789012-abc.apps.googleusercontent.com",
            "issuer": issuer,
            "gmail": GOOGLE_GMAIL,
            "calendar": GOOGLE_CALENDAR,
        })
    }

    async fn started(config: &Value, secret: Option<&str>) -> anyhow::Result<ConnectorRuntime> {
        let context = InstanceContext {
            secret,
            ..InstanceContext::default()
        };
        GoogleWorkspaceConnector.instantiate(config, context).await
    }

    #[tokio::test]
    async fn it_starts_with_its_secret_and_writes_only_when_let() {
        let config = declared("https://accounts.google.com");
        let runtime = started(&config, Some("GOCSPX-secret")).await.unwrap();
        let names: Vec<String> = runtime
            .tools
            .iter()
            .map(|t| t.info().name.clone())
            .collect();
        // Read only, unless the company lets its tools write.
        assert!(!GoogleWorkspaceConfig::read(&config).unwrap().write_tools);
        assert_eq!(names, GOOGLE_WORKSPACE_TOOLS[..3]);
        let mut writing = config;
        writing["writeTools"] = json!(true);
        let runtime = started(&writing, Some("GOCSPX-secret")).await.unwrap();
        assert_eq!(runtime.tools.len(), GOOGLE_WORKSPACE_TOOLS.len());
    }

    #[tokio::test]
    async fn it_does_not_start_without_its_secret_its_network_or_its_declaration() {
        let without_secret = started(&declared("https://accounts.google.com"), None).await;
        assert!(
            without_secret
                .err()
                .unwrap()
                .to_string()
                .contains("secret de l’application")
        );
        // An address the operator's network policy no longer allows.
        let private = started(&declared("http://127.0.0.1:9"), Some("s")).await;
        assert!(private.err().unwrap().to_string().contains("https://"));
        let unreadable = started(&json!({ "clientId": 1 }), Some("s")).await;
        assert!(
            unreadable
                .err()
                .unwrap()
                .to_string()
                .contains("Déclaration Google Workspace illisible")
        );
    }
}
