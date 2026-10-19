//! Model Context Protocol: the bridge exposing MCP tools as platform tools,
//! the connector type, how its servers are reached, how their failures are
//! explained (token masked), and a demo server.

mod bridge;
mod connector;
mod demo_docs;
mod failure;
mod network;
mod redact;

pub use bridge::{
    CONNECTOR_USE, MAX_TOOLS, McpClient, McpToolsetOptions, create_mcp_toolset, mcp_tool_name,
    tool_namespace,
};
pub use connector::{McpConnector, McpConnectorConfig};
pub use demo_docs::DemoDocsServer;
pub use failure::{ACCESS_DENIED, NOT_MCP, NOT_PUBLIC, TOO_SLOW, UNREACHABLE};
pub use network::{MAX_URL_CHARS, NetworkPolicy, http_client, is_public_ip, url_problem};
pub use redact::{MASK, Redactor};
