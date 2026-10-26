//! What the administration may change of a connector's configuration: the
//! address of an MCP server it added (ADR-025), the address, token header
//! and operations of a REST API (ADR-112). The demo connector and the
//! servers the operator wrote change only their name and activation here.
//! Tool prefixes never overlap within a kind, operator-written servers
//! included.

use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_connectors::{mcp, rest};
use mimorya_core::DomainError;
use mimorya_db::repositories::connectors::{ConnectorInstanceRow, list_connector_instances};
use serde_json::{Value, json};
use sqlx::PgConnection;

use super::connector_rules::PolicyScope;

/// A REST API's changes: `None` keeps what is stored.
#[derive(Default)]
pub struct RestChanges {
    /// `Some(None)`: back to `Authorization: Bearer`.
    pub token_header: Option<Option<String>>,
    pub operations: Option<Vec<rest::RestOperation>>,
}

/// A configuration the administration may change.
pub enum Editable {
    /// An MCP server added over HTTPS.
    Mcp {
        prefix: String,
        url: String,
    },
    Rest(rest::RestConfig),
}

impl Editable {
    /// `None` for the demo connector, an operator's server (stdio) or a
    /// REST declaration no longer readable.
    pub fn of(instance: &ConnectorInstanceRow) -> Option<Self> {
        let config = &instance.config;
        match instance.connector_type.as_str() {
            "mcp" if config["transport"] == "http" => Some(Self::Mcp {
                prefix: config["prefix"].as_str().unwrap_or_default().to_string(),
                url: config["url"].as_str().unwrap_or_default().to_string(),
            }),
            "rest" => serde_json::from_value(config.clone()).ok().map(Self::Rest),
            _ => None,
        }
    }

    pub fn url(&self) -> &str {
        match self {
            Self::Mcp { url, .. } => url,
            Self::Rest(config) => &config.url,
        }
    }

    pub fn token_header(&self) -> Option<&str> {
        match self {
            Self::Mcp { .. } => None,
            Self::Rest(config) => config.token_header.as_deref(),
        }
    }

    /// The tools of a REST API whose operation these operations remove or
    /// change in method, path or parameters: what a policy allowed of them
    /// no longer holds (a read become a deletion, a filter become a write).
    pub fn redeclared(&self, operations: &[rest::RestOperation]) -> Vec<String> {
        let Self::Rest(config) = self else {
            return Vec::new();
        };
        config
            .operations
            .iter()
            .filter(|before| {
                !operations.iter().any(|after| {
                    after.name == before.name
                        && after.method == before.method
                        && after.path == before.path
                        && after.parameters == before.parameters
                })
            })
            .map(|before| rest::rest_tool_name(&config.prefix, &before.name))
            .collect()
    }

    /// Which tools its policies may name, and how.
    pub fn policy_scope(&self) -> PolicyScope {
        match self {
            Self::Mcp { prefix, .. } => PolicyScope {
                namespace: mcp::tool_namespace(prefix),
                replaceable_descriptions: true,
            },
            // An operation's description is written in its declaration.
            Self::Rest(config) => PolicyScope {
                namespace: rest::tool_namespace(&config.prefix),
                replaceable_descriptions: false,
            },
        }
    }

    /// The configuration to store with these changes, checked for a REST
    /// API as a whole. The address is checked by the caller for an MCP server.
    pub fn changed(
        self,
        url: Option<String>,
        changes: Option<RestChanges>,
        network: NetworkPolicy,
    ) -> Result<Value, DomainError> {
        match self {
            Self::Mcp {
                prefix,
                url: current,
            } => {
                if changes.is_some() {
                    return Err(DomainError::validation(
                        "Seule une API REST a un en-tête de jeton et des opérations",
                    ));
                }
                let url = url.unwrap_or(current);
                Ok(json!({ "transport": "http", "prefix": prefix, "url": url }))
            }
            Self::Rest(mut config) => {
                if let Some(url) = url {
                    config.url = url;
                }
                let changes = changes.unwrap_or_default();
                if let Some(header) = changes.token_header {
                    config.token_header = header;
                }
                if let Some(operations) = changes.operations {
                    config.operations = operations;
                }
                if let Some(problem) = rest::config_problem(&config, network) {
                    return Err(DomainError::validation(problem));
                }
                serde_json::to_value(&config)
                    .map_err(|_| DomainError::validation("Déclaration illisible"))
            }
        }
    }
}

/// Whether the administration added this connector, and may delete it.
pub fn added_here(instance: &ConnectorInstanceRow) -> bool {
    instance.connector_type == "rest"
        || (instance.connector_type == "mcp" && instance.config["transport"] == "http")
}

/// The namespace of a kind's tools under this prefix.
fn namespace(connector_type: &str, prefix: &str) -> Option<String> {
    match connector_type {
        "mcp" => Some(mcp::tool_namespace(prefix)),
        "rest" => Some(rest::tool_namespace(prefix)),
        _ => None,
    }
}

/// Whether another connector of this kind already names its tools with a
/// prefix that overlaps this one.
pub async fn prefix_taken(
    conn: &mut PgConnection,
    connector_type: &str,
    prefix: &str,
) -> anyhow::Result<bool> {
    let Some(wanted) = namespace(connector_type, prefix) else {
        return Ok(false);
    };
    Ok(list_connector_instances(conn, false)
        .await?
        .iter()
        .filter(|instance| instance.connector_type == connector_type)
        .filter_map(|instance| instance.config["prefix"].as_str())
        .filter_map(|other| namespace(connector_type, other))
        .any(|other| other.starts_with(&wanted) || wanted.starts_with(&other)))
}

/// The names of the operations, for the audit.
pub fn operation_names(operations: &[rest::RestOperation]) -> Vec<&str> {
    operations.iter().map(|o| o.name.as_str()).collect()
}
