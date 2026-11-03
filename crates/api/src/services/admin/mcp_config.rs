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

/// Whether `instance` is an MCP server added here whose OAuth declaration
/// cannot be read.
pub fn oauth_unreadable(instance: &ConnectorInstanceRow) -> bool {
    let config = &instance.config;
    instance.connector_type == "mcp"
        && config["transport"] == "http"
        && config
            .get("oauth")
            .filter(|oauth| !oauth.is_null())
            .is_some_and(|oauth| serde_json::from_value::<OAuthDeclaration>(oauth.clone()).is_err())
}
