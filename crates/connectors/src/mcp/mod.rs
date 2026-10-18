//! Model Context Protocol: the bridge exposing MCP tools as platform tools,
//! the connector type, and a demo server.

mod bridge;
mod connector;
mod demo_docs;

pub use bridge::{McpClient, McpToolsetOptions, create_mcp_toolset, mcp_tool_name};
pub use connector::{McpConnector, McpConnectorConfig};
pub use demo_docs::DemoDocsServer;
