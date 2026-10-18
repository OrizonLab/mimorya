//! Runs the demo documentation MCP server over stdio (FICTIONAL data).

use mimorya_connectors::mcp::DemoDocsServer;
use rmcp::ServiceExt;
use rmcp::transport::stdio;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    DemoDocsServer::default()
        .serve(stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}
