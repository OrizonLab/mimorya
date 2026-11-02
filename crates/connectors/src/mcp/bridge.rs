//! MCP bridge: exposes the tools of an MCP server as platform tools.
//!
//! MCP servers know nothing about our users, so the bridge is conservative:
//! every MCP tool defaults to risk `write` (human validation required) and to
//! the `connector:use` permission, until an administrator maps it explicitly
//! (connector `tool_policies`). Results are untrusted data like any tool output.
//!
//! What a server declares is read and checked by `declarations`; what its
//! tools return has the token masked. A call, its `input_required` rounds
//! played, is bounded by its delay; one that failed without the server's
//! answer has the server pinged (`liveness`, ADR-164).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Tool, ToolContext, ToolInfo, ToolRef, ToolResult};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceError};
use serde_json::{Value, json};
use tokio::time::Instant;

use super::declarations::declared_tools;
use super::failure::call_failed;
use super::liveness::Liveness;
use crate::outbound::{CONNECTOR_USE, Redactor, too_slow};
use crate::types::DEFAULT_CALL_TIMEOUT;

pub type McpClient = RunningService<RoleClient, ()>;

#[derive(Debug, Clone)]
pub struct McpToolsetOptions {
    /// Short slug used to namespace tool names, e.g. "docs" → "mcp_docs_search".
    pub prefix: String,
    pub default_risk: ToolRisk,
    pub default_permissions: Vec<String>,
    /// Masks the server's token in what it declares and its tools return.
    pub redactor: Redactor,
    /// How long a call may take, its answer read.
    pub call_timeout: Duration,
    /// Whether the server still answers, shared by the tools and read by
    /// the session (ADR-164).
    pub liveness: Arc<Liveness>,
}

impl McpToolsetOptions {
    pub fn new(prefix: impl Into<String>) -> Self {
        let prefix = prefix.into();
        Self {
            liveness: Arc::new(Liveness::new(prefix.clone())),
            prefix,
            default_risk: ToolRisk::Write,
            default_permissions: vec![CONNECTOR_USE.into()],
            redactor: Redactor::default(),
            call_timeout: DEFAULT_CALL_TIMEOUT,
        }
    }
}

struct McpTool {
    info: ToolInfo,
    remote_name: String,
    prefix: String,
    input_schema: Value,
    client: Arc<McpClient>,
    redactor: Redactor,
    call_timeout: Duration,
    liveness: Arc<Liveness>,
}

#[async_trait]
impl Tool for McpTool {
    fn info(&self) -> &ToolInfo {
        &self.info
    }

    fn input_schema(&self) -> Value {
        self.input_schema.clone()
    }

    /// The MCP server validates the arguments; only their shape is checked here.
    fn parse_input(&self, raw: &Value) -> Result<Value, Vec<String>> {
        if raw.is_object() {
            Ok(raw.clone())
        } else {
            Err(vec!["(racine) : objet attendu".into()])
        }
    }

    fn describe_action(&self, input: &Value) -> Option<String> {
        Some(format!(
            "Appeler l’outil MCP « {} » ({}) avec {input}",
            self.remote_name, self.prefix
        ))
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        let arguments = match input {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        let params = CallToolRequestParams::new(self.remote_name.clone()).with_arguments(arguments);
        // A ping is given as long as a call; a server lost after this call
        // ended when it was made.
        let (ping, asked) = (self.call_timeout, Instant::now());
        let result = match self.call(params).await {
            Ok(result) => result,
            Err(ServiceError::Timeout { .. }) => {
                self.liveness.after_failure(&self.client, None, ping, asked);
                return Err(too_slow(self.call_timeout));
            }
            Err(error) => {
                self.liveness
                    .after_failure(&self.client, Some(&error), ping, asked);
                return Err(call_failed(&error, &self.redactor));
            }
        };
        let text: Vec<String> = result
            .content
            .iter()
            .map(|part| match part.as_text() {
                Some(text) => self.redactor.text(&text.text),
                None => {
                    let kind = serde_json::to_value(part)
                        .ok()
                        .and_then(|v| v["type"].as_str().map(str::to_string));
                    format!("[contenu {}]", kind.unwrap_or_else(|| "inconnu".into()))
                }
            })
            .collect();
        let is_error = result.is_error == Some(true);
        Ok(ToolResult {
            content: match result.structured_content {
                Some(content) => self.redactor.value(content),
                None => json!(text.join("\n")),
            },
            is_error,
            summary: Some(if is_error {
                format!("Erreur MCP ({})", self.remote_name)
            } else {
                format!("Réponse MCP ({})", self.remote_name)
            }),
            // Neither citations nor quality criteria: a server only answers.
            ..ToolResult::default()
        })
    }
}

impl McpTool {
    /// The call, its `input_required` rounds played by the MCP library,
    /// bounded by its delay. The library takes no delay for it, nor gives
    /// its request's identifier: the server is not told the call was
    /// abandoned (ADR-164).
    async fn call(&self, params: CallToolRequestParams) -> Result<CallToolResult, ServiceError> {
        let timeout = self.call_timeout;
        match tokio::time::timeout(timeout, self.client.call_tool(params)).await {
            Ok(answer) => answer,
            Err(_) => Err(ServiceError::Timeout { timeout }),
        }
    }
}

/// What the model reads of an MCP tool: the server it comes from, then its
/// description (as declared, or as an administrator replaced it, ADR-057).
pub fn mcp_tool_description(prefix: &str, description: &str) -> String {
    format!("[Serveur MCP « {prefix} »] {description}")
}

/// The tools of a connected MCP server, with the conservative defaults.
pub async fn create_mcp_toolset(
    client: Arc<McpClient>,
    options: &McpToolsetOptions,
) -> anyhow::Result<Vec<ToolRef>> {
    let declared = declared_tools(&client, &options.prefix, &options.redactor).await?;
    let mut result: Vec<ToolRef> = Vec::with_capacity(declared.len());
    for tool in declared {
        let description = mcp_tool_description(&options.prefix, &tool.description);
        let info = ToolInfo::try_new(&tool.name, &tool.title, &description, options.default_risk)
            .map_err(anyhow::Error::msg)?
            .requires(options.default_permissions.clone());
        result.push(Arc::new(McpTool {
            info,
            remote_name: tool.remote_name,
            prefix: options.prefix.clone(),
            input_schema: tool.input_schema,
            client: client.clone(),
            redactor: options.redactor.clone(),
            call_timeout: options.call_timeout,
            liveness: options.liveness.clone(),
        }));
    }
    Ok(result)
}
