//! The "MCP server" connector type: connects a server over Streamable HTTP
//! (what companies configure, ADR-025) or stdio (a child process, written by
//! the operator of a private deployment only) and exposes its tools through
//! the bridge. Connecting and listing the tools are bounded in time.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use async_trait::async_trait;
use rmcp::ServiceExt;
use rmcp::service::RunningServiceCancellationToken;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{ConfigureCommandExt, StreamableHttpClientTransport, TokioChildProcess};
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;

use super::bridge::{McpClient, McpToolsetOptions, create_mcp_toolset};
use super::network::{http_client, url_problem};
use crate::types::{Connector, ConnectorCloser, ConnectorInfo, ConnectorRuntime, InstanceContext};

/// Connecting to the server and listing its tools, together.
const START_TIMEOUT: Duration = Duration::from_secs(20);

/// No `Debug`: `env` may carry credentials of the MCP server.
#[derive(Clone, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case", deny_unknown_fields)]
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
        context: InstanceContext<'_>,
    ) -> anyhow::Result<ConnectorRuntime> {
        let config: McpConnectorConfig = serde_json::from_value(config.clone())?;
        let started = tokio::time::timeout(START_TIMEOUT, start(config, context))
            .await
            .context("MCP server: no answer in time")??;
        Ok(started)
    }
}

async fn start(
    config: McpConnectorConfig,
    context: InstanceContext<'_>,
) -> anyhow::Result<ConnectorRuntime> {
    let (client, prefix): (McpClient, String) = match config {
        McpConnectorConfig::Stdio {
            prefix,
            command,
            args,
            env,
        } => {
            if !context.network.allow_stdio {
                bail!("stdio MCP servers are not allowed on this server");
            }
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
            // Checked again here: the operator may have narrowed the policy.
            if let Some(problem) = url_problem(&url, context.network) {
                bail!("{problem}");
            }
            let mut config = StreamableHttpClientTransportConfig::with_uri(url);
            if let Some(token) = context.secret {
                config = config.auth_header(token);
            }
            let transport =
                StreamableHttpClientTransport::with_client(http_client(context.network)?, config);
            (().serve(transport).await?, prefix)
        }
    };
    let token = client.cancellation_token();
    let tools = create_mcp_toolset(Arc::new(client), &McpToolsetOptions::new(prefix)).await;
    let closer = Cancel(std::sync::Mutex::new(Some(token)));
    match tools {
        Ok(tools) => Ok(ConnectorRuntime {
            tools,
            closer: Some(Box::new(closer)),
        }),
        Err(error) => {
            closer.close().await;
            Err(error)
        }
    }
}
