//! Model Context Protocol: the bridge exposing MCP tools as platform tools,
//! the connector type (its servers reached as `outbound` says), how their
//! failures are explained (token masked), and a demo server.

mod bridge;
mod connector;
mod declarations;
mod demo_docs;
mod failure;
mod visible;

pub use bridge::{McpClient, McpToolsetOptions, create_mcp_toolset, mcp_tool_description};
pub use connector::{McpConnector, McpConnectorConfig};
pub use declarations::{
    MAX_DECLARATIONS_BYTES, MAX_DESCRIPTION_CHARS, MAX_TOOLS, mcp_tool_name, tool_namespace,
};
pub use demo_docs::DemoDocsServer;
pub use failure::NOT_MCP;
