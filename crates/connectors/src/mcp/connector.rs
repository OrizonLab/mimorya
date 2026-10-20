//! The "MCP server" connector type: connects a server over Streamable HTTP
//! (what companies configure, ADR-025) or stdio (a child process, written by
//! the operator of a private deployment only) and exposes its tools through
//! the bridge. Connecting and listing the tools are bounded in time; a
//! failure is explained in French, without the token (`failure`, `redact`).
//! A session whose transport closed (a local server that exited) says so,
//! for the platform to start it again; an HTTP server that restarts is
//! reached again by the client itself (`reinit_on_expired_session`).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::bail;
use async_trait::async_trait;
use rmcp::ServiceExt;
use rmcp::service::RunningServiceCancellationToken;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{ConfigureCommandExt, StreamableHttpClientTransport, TokioChildProcess};
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;

use super::bridge::{McpClient, McpToolsetOptions, create_mcp_toolset};
use super::failure::{TOO_SLOW, start_failed};
use super::network::{http_client, url_problem};
use super::redact::Redactor;
use super::token::token_problem;
use crate::types::{Connector, ConnectorCloser, ConnectorInfo, ConnectorRuntime, InstanceContext};

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

/// The session with the server: cancelled once, closed when its transport is.
struct Session {
    client: Arc<McpClient>,
    cancel: std::sync::Mutex<Option<RunningServiceCancellationToken>>,
}

#[async_trait]
impl ConnectorCloser for Session {
    async fn close(&self) {
        if let Some(token) = self.cancel.lock().expect("closer").take() {
            token.cancel();
        }
    }

    fn is_closed(&self) -> bool {
        self.client.is_transport_closed()
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
        // Connecting to the server and listing its tools, together.
        match tokio::time::timeout(context.start_timeout, start(config, context)).await {
            Ok(started) => started,
            Err(_) => {
                let seconds = context.start_timeout.as_secs_f32().to_string();
                bail!("{TOO_SLOW} (délai de {} s)", seconds.replace('.', ","))
            }
        }
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
                bail!("Les serveurs lancés en local (stdio) ne sont pas autorisés sur ce serveur");
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
            let process = TokioChildProcess::new(command).map_err(|error| {
                anyhow::anyhow!("Le programme du serveur MCP n’a pas pu être lancé ({error})")
            })?;
            let client = ()
                .serve(process)
                .await
                .map_err(|error| start_failed(&error.into(), &Redactor::default()))?;
            (client, prefix)
        }
        McpConnectorConfig::Http { prefix, url } => {
            // Checked again here: the operator may have narrowed the policy.
            if let Some(problem) = url_problem(&url, context.network) {
                bail!("{problem}");
            }
            // A token saved before the rule may hold what the masking cannot follow.
            if context
                .secret
                .is_some_and(|token| token_problem(token).is_some())
            {
                bail!(
                    "Le jeton enregistré n’a plus un format accepté (RFC 6750) : saisissez un \
                     jeton conforme"
                );
            }
            let mut config = StreamableHttpClientTransportConfig::with_uri(url);
            if let Some(token) = context.secret {
                config = config.auth_header(token);
            }
            let transport =
                StreamableHttpClientTransport::with_client(http_client(context.network)?, config);
            let client = ()
                .serve(transport)
                .await
                .map_err(|error| start_failed(&error.into(), &Redactor::new(context.secret)))?;
            (client, prefix)
        }
    };
    let token = client.cancellation_token();
    let options = McpToolsetOptions {
        redactor: Redactor::new(context.secret),
        ..McpToolsetOptions::new(prefix)
    };
    let client = Arc::new(client);
    // Its messages may quote what the server declared.
    let tools = create_mcp_toolset(client.clone(), &options)
        .await
        .map_err(|error| anyhow::anyhow!(options.redactor.text(&format!("{error:#}"))));
    let closer = Session {
        client,
        cancel: std::sync::Mutex::new(Some(token)),
    };
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
