//! Model Context Protocol: the bridge exposing MCP tools as platform tools,
//! the connector type (its servers reached as `outbound` says, their answers
//! read within bounds by `http`), how their failures are explained (token
//! masked), and a demo server.

mod bridge;
mod connector;
mod declarations;
mod demo_docs;
mod failure;
mod http;
mod visible;

pub use bridge::{McpClient, McpToolsetOptions, create_mcp_toolset, mcp_tool_description};
pub use connector::{McpConnector, McpConnectorConfig};
pub use declarations::{
    MAX_DECLARATIONS_BYTES, MAX_DESCRIPTION_CHARS, MAX_TOOLS, mcp_tool_name, tool_namespace,
};
pub use demo_docs::DemoDocsServer;
pub use failure::NOT_MCP;
pub use http::{BoundedHttpClient, MAX_RESPONSE_BYTES, TOO_LONG};
