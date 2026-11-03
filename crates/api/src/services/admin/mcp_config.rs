//! What an MCP server added over HTTPS stores (ADR-025): its prefix, its
//! address, and the OAuth declared on it (ADR-171). A declaration that can
//! no longer be read, which only a hand or an older process could leave,
//! is told apart, so that it can be withdrawn or declared again.

use mimorya_connectors::mcp::oauth::OAuthDeclaration;
use mimorya_db::repositories::connectors::ConnectorInstanceRow;
use serde_json::{Value, json};

/// The stored configuration of an MCP server added over HTTPS.
pub fn mcp_config(prefix: &str, url: &str, oauth: Option<&OAuthDeclaration>) -> Value {
    let mut config = json!({ "transport": "http", "prefix": prefix, "url": url });
    if let Some(oauth) = oauth {
        config["oauth"] = json!(oauth);
    }
    config
}

/// A declaration stored but no longer readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

/// The OAuth declared in a stored MCP configuration, if any: the one
/// reading of it.
pub fn stored_oauth(config: &Value) -> Result<Option<OAuthDeclaration>, Unreadable> {
    match config.get("oauth").filter(|oauth| !oauth.is_null()) {
        None => Ok(None),
        Some(oauth) => serde_json::from_value(oauth.clone())
            .map(Some)
            .map_err(|_| Unreadable),
    }
}

/// Whether `instance` is an MCP server added here whose OAuth declaration
/// cannot be read.
pub fn oauth_unreadable(instance: &ConnectorInstanceRow) -> bool {
    instance.connector_type == "mcp"
        && instance.config["transport"] == "http"
        && stored_oauth(&instance.config).is_err()
}
