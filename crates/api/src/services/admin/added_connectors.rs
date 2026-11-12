//! The connectors a company's administrators add, MCP servers over HTTPS,
//! REST APIs (ADR-112), the applications people link their accounts at
//! (ADR-116, ADR-121) and the standard mail (ADR-205): which of them the
//! administration may delete, the prefixes naming their tools, which never
//! overlap within a kind, operator-written servers included, a REST API's
//! operations as the audit names them, and why the configuration of one
//! no longer readable, or written by the operator, does not change here.

use mimorya_connectors::standard_mail::MAIL_SERVER;
use mimorya_connectors::{mcp, rest};
use mimorya_core::account_links::LinkKind;
use mimorya_db::repositories::connectors::{ConnectorInstanceRow, list_connector_instances};
use sqlx::PgConnection;

use super::mcp_config::oauth_unreadable;

/// Whether the administration added this connector, and may delete it.
pub fn added_here(instance: &ConnectorInstanceRow) -> bool {
    instance.connector_type == "rest"
        || instance.connector_type == MAIL_SERVER
        || LinkKind::of(&instance.connector_type).is_some()
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

/// Why the administration may not change this connector's configuration,
/// which it cannot read (in French).
pub fn unchangeable(instance: &ConnectorInstanceRow) -> String {
    let kind = instance.connector_type.as_str();
    if kind == "rest" {
        "Déclaration de cette API illisible : supprimez-la puis ajoutez-la de nouveau".into()
    } else if oauth_unreadable(instance) {
        "Déclaration OAuth de ce serveur illisible : retirez-la, puis déclarez-la de nouveau".into()
    } else if kind == MAIL_SERVER {
        "Déclaration de la messagerie illisible : supprimez le connecteur puis ajoutez-le de \
         nouveau"
            .into()
    } else if let Some(link) = LinkKind::of(kind) {
        format!(
            "Déclaration {} illisible : supprimez le connecteur puis ajoutez-le de nouveau",
            link.product()
        )
    } else {
        "Ce connecteur est réglé par l’exploitant : seuls son nom et son activation changent ici"
            .into()
    }
}
