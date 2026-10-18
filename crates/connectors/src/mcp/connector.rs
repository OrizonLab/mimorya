//! The "MCP server" connector type: connects a server over stdio (child
//! process) or Streamable HTTP and exposes its tools through the bridge.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rmcp::ServiceExt;
use rmcp::service::RunningServiceCancellationToken;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{ConfigureCommandExt, StreamableHttpClientTransport, TokioChildProcess};
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;

use super::bridge::{McpToolsetOptions, create_mcp_toolset};
use crate::types::{Connector, ConnectorCloser, ConnectorInfo, ConnectorRuntime};

/// No `Debug`: `env` may carry credentials of the MCP server.
#[derive(Clone, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum McpConnectorConfig {
    Stdio {
        prefix: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    Http {
        prefix: String,
        url: String,
    },
}

/// Variables passed to a child process besides the configured ones.
const INHERITED_ENV: [&str; 2] = ["PATH", "HOME"];

struct Cancel(std::sync::Mutex<Option<RunningServiceCancellationToken>>);

#[async_trait]
impl ConnectorCloser for Cancel {
    async fn close(&self) {
        if let Some(token) = self.0.lock().expect("closer").take() {
            token.cancel();
        }
    }
}

pub struct McpConnector;

#[async_trait]
impl Connector for McpConnector {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            connector_type: "mcp",
            name: "Serveur MCP",
            description: "Connecte un serveur Model Context Protocol. Chaque outil exposé doit être associé à des permissions et à un niveau de risque par l’administrateur.",
            demo: false,
            permissions: &[],
        }
    }

    async fn instantiate(
        &self,
        config: &Value,
        secret: Option<&str>,
    ) -> anyhow::Result<ConnectorRuntime> {
        let config: McpConnectorConfig = serde_json::from_value(config.clone())?;
        let (client, prefix) = match config {
            McpConnectorConfig::Stdio {
                prefix,
                command,
                args,
                env,
            } => {
                let command = Command::new(command).configure(|cmd| {
                    cmd.args(&args);
                    // Only explicitly configured variables (plus PATH/HOME) reach the child.
                    cmd.env_clear();
                    for key in INHERITED_ENV {
                        if let Ok(value) = std::env::var(key) {
                            cmd.env(key, value);
                        }
                    }
                    cmd.envs(&env);
                });
                (().serve(TokioChildProcess::new(command)?).await?, prefix)
            }
            McpConnectorConfig::Http { prefix, url } => {
                let mut config = StreamableHttpClientTransportConfig::with_uri(url);
                if let Some(token) = secret {
                    config = config.auth_header(token);
                }
                let transport = StreamableHttpClientTransport::from_config(config);
                (().serve(transport).await?, prefix)
            }
        };
        let token = client.cancellation_token();
        let tools = create_mcp_toolset(Arc::new(client), &McpToolsetOptions::new(prefix)).await?;
        Ok(ConnectorRuntime {
            tools,
            closer: Some(Box::new(Cancel(std::sync::Mutex::new(Some(token))))),
        })
    }
}
