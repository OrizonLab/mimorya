//! The "MCP server" connector type: connects a server over Streamable HTTP
//! (what companies configure, ADR-025) or stdio (a child process, written by
//! the operator of a private deployment only) and exposes its tools through
//! the bridge. Connecting and listing the tools are bounded in time; a
//! failure is explained in French, without the token (`failure`, `redact`);
//! what an HTTP server answers is read within bounds (`http`, ADR-161).
//! A session whose transport closed (a local server that exited), or whose
//! server did not answer a ping after a failed call (`liveness`, ADR-164),
//! says so, and when it ended (ADR-165), for the platform to start it again; an HTTP server that
//! restarts and says so (404) is reached again by the client itself
//! (`reinit_on_expired_session`). A server declared with OAuth is sent the
//! access token of its authorization, and only it; its session is retired
//! before the token expires (`expiry`, ADR-171).

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
use tokio::time::Instant;

use super::bridge::{McpClient, McpToolsetOptions, create_mcp_toolset};
use super::expiry::{Expiry, OAUTH_UNUSABLE};
use super::failure::{TokenKind, start_failed};
use super::http::{BoundedHttpClient, TOO_LONG};
use super::liveness::Liveness;
use super::oauth::{
    OAUTH_EACH_PERSON, OAUTH_TO_AUTHORIZE, OAuthDeclaration, OAuthHolder, access_token_problem,
};
use crate::outbound::{Redactor, http_client, token_problem, too_slow, url_problem};
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
        /// Declared by an administrator (ADR-171).
        #[serde(default)]
        oauth: Option<OAuthDeclaration>,
    },
}

/// Variables passed to a child process besides the configured ones.
const INHERITED_ENV: [&str; 2] = ["PATH", "HOME"];

/// The session with the server: cancelled once, closed when its transport
/// is, its server did not answer a ping, or its token is about to expire.
struct Session {
    client: Arc<McpClient>,
    cancel: std::sync::Mutex<Option<RunningServiceCancellationToken>>,
    liveness: Arc<Liveness>,
    expiry: Expiry,
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
            || self.liveness.is_lost()
            || self.expiry.passed(Instant::now()).is_some()
    }

    fn ended_at(&self) -> Option<Instant> {
        self.liveness
            .ended_at()
            .or_else(|| self.expiry.passed(Instant::now()))
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
            reaches_outside: true,
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
            Err(_) => Err(too_slow(context.start_timeout)),
        }
    }
}

async fn start(
    config: McpConnectorConfig,
    context: InstanceContext<'_>,
) -> anyhow::Result<ConnectorRuntime> {
    let mut kind = TokenKind::Typed;
    let mut expiry = Expiry::default();
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
                .map_err(|error| start_failed(&error.into(), &Redactor::default(), kind))?;
            (client, prefix)
        }
        McpConnectorConfig::Http { prefix, url, oauth } => {
            // Checked again here: the operator may have narrowed the policy.
            if let Some(problem) = url_problem(&url, context.network) {
                bail!("{problem}");
            }
            // Its token is its authorization's, given by an administrator
            // (ADR-171), or by each person for themself (ADR-174): none,
            // nothing starts.
            if let Some(declared) = &oauth {
                if context.secret.is_none() {
                    match declared.holder {
                        OAuthHolder::Company => bail!(OAUTH_TO_AUTHORIZE),
                        OAuthHolder::Person => bail!(OAUTH_EACH_PERSON),
                    }
                }
                kind = TokenKind::Authorization;
                expiry =
                    Expiry::of(context.secret_until, Instant::now()).map_err(anyhow::Error::msg)?;
            }
            // A token saved before the rule may hold what the masking cannot
            // follow; an authorization's is held to the rule it was kept by.
            match (kind, context.secret) {
                (TokenKind::Typed, Some(token)) if token_problem(token).is_some() => bail!(
                    "Le jeton enregistré n’a plus un format accepté (RFC 6750) : saisissez un \
                     jeton conforme"
                ),
                (TokenKind::Authorization, Some(token))
                    if access_token_problem(token).is_some() =>
                {
                    bail!(OAUTH_UNUSABLE)
                }
                _ => {}
            }
            let mut config = StreamableHttpClientTransportConfig::with_uri(url);
            if let Some(token) = context.secret {
                config = config.auth_header(token);
            }
            // Its answers read within bounds (ADR-161).
            let client = BoundedHttpClient::new(http_client(context.network)?);
            let cuts = client.cuts();
            let transport = StreamableHttpClientTransport::with_client(client, config);
            let client = ().serve(transport).await.map_err(|error| {
                // The library does not say why it gave up: the client does.
                if cuts.any() {
                    anyhow::anyhow!(TOO_LONG)
                } else {
                    start_failed(&error.into(), &Redactor::new(context.secret), kind)
                }
            })?;
            (client, prefix)
        }
    };
    let token = client.cancellation_token();
    let options = McpToolsetOptions {
        redactor: Redactor::new(context.secret),
        token: kind,
        call_timeout: context.call_timeout,
        ..McpToolsetOptions::new(prefix)
    };
    let client = Arc::new(client);
    // Its messages may quote what the server declared.
    let tools = create_mcp_toolset(client.clone(), &options)
        .await
        .map_err(|error| anyhow::anyhow!(options.redactor.text(&format!("{error:#}"))));
    // Its life counted from now, as the platform counts it (ADR-163): a
    // slow start may have used up what the token had left.
    let tools = tools.and_then(|tools| {
        Expiry::of(context.secret_until, Instant::now())
            .map(|_| tools)
            .map_err(anyhow::Error::msg)
    });
    let closer = Session {
        client,
        cancel: std::sync::Mutex::new(Some(token)),
        liveness: options.liveness.clone(),
        expiry,
    };
    match tools {
        Ok(tools) => {
            options.liveness.watch(&closer.client);
            Ok(ConnectorRuntime {
                tools,
                closer: Some(Box::new(closer)),
            })
        }
        Err(error) => {
            closer.close().await;
            Err(error)
        }
    }
}
