//! Model Context Protocol: the bridge exposing MCP tools as platform tools,
//! the connector type, how its servers are reached, and a demo server.

mod bridge;
mod connector;
mod demo_docs;
mod network;

pub use bridge::{McpClient, McpToolsetOptions, create_mcp_toolset, mcp_tool_name};
pub use connector::{McpConnector, McpConnectorConfig};
pub use demo_docs::DemoDocsServer;
pub use network::{MAX_URL_CHARS, NetworkPolicy, http_client, is_public_ip, url_problem};
