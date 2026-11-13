//! The "standard mail" connector type (ADR-205): the company's own mail
//! servers, declared once, where each person links their own account. Its
//! declaration is checked again against the network policy at each start,
//! which the operator may have narrowed since it was saved. Its tools read
//! the mailbox of the person a run serves, with the account they linked.

use anyhow::bail;
use async_trait::async_trait;
use mimorya_core::tools::typed_tool;
use serde_json::Value;

use super::tools::{MailReach, MailRead, MailSearch};
use super::{MAIL_SERVER, MailServerConfig};
use crate::types::{Connector, ConnectorInfo, ConnectorRuntime, InstanceContext};

pub struct MailServerConnector;

#[async_trait]
impl Connector for MailServerConnector {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            connector_type: MAIL_SERVER,
            name: "Messagerie standard",
            description: "La boîte aux lettres et l’agenda de chacun sur les serveurs de messagerie de l’entreprise (IMAP, SMTP, CalDAV), que chaque personne relie elle-même avec son identifiant et son mot de passe : l’agent n’y lit que ce que la personne servie peut lire, et n’y écrit, si l’entreprise le permet, que ce qui est validé.",
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
        let config = MailServerConfig::read(config)?;
        if let Some(problem) = config.problem(context.network) {
            bail!("{problem}");
        }
        let reach = MailReach {
            network: context.network,
            call_timeout: context.call_timeout,
        };
        Ok(ConnectorRuntime {
            tools: vec![
                typed_tool(MailSearch { reach }),
                typed_tool(MailRead { reach }),
            ],
            closer: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn starts_on_a_declaration_the_network_policy_still_allows() {
        let declared = json!({
            "imap": { "host": "imap.example.com", "port": 993 },
            "smtp": { "host": "smtp.example.com", "port": 465, "security": "tls" },
        });
        let context = InstanceContext::default();
        let started = MailServerConnector.instantiate(&declared, context).await;
        let names: Vec<String> = started
            .unwrap()
            .tools
            .iter()
            .map(|tool| tool.info().name.clone())
            .collect();
        assert_eq!(names, ["mail_search", "mail_read"]);
        let mut narrowed = declared.clone();
        narrowed["imap"]["host"] = json!("10.1.2.3");
        let refused = MailServerConnector.instantiate(&narrowed, context).await;
        let said = refused.err().unwrap().to_string();
        assert!(said.starts_with("Serveur IMAP"), "{said}");
        let unreadable = MailServerConnector
            .instantiate(&json!({ "imap": "imap.example.com" }), context)
            .await;
        assert!(unreadable.err().unwrap().to_string().contains("illisible"));
    }
}
