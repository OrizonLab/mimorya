//! MCP bridge: exposes the tools of an MCP server as platform tools.
//!
//! MCP servers know nothing about our users, so the bridge is conservative:
//! every MCP tool defaults to risk `write` (human validation required) and to
//! the `connector:use` permission, until an administrator maps it explicitly
//! (connector `tool_policies`). Results are untrusted data like any tool output.
//!
//! What a server declares is read and checked by `declarations`; what its
//! tools return has the token masked.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Tool, ToolContext, ToolInfo, ToolRef, ToolResult};
use rmcp::RoleClient;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use serde_json::{Value, json};

use super::declarations::declared_tools;
use super::failure::call_failed;
use super::redact::Redactor;

pub type McpClient = RunningService<RoleClient, ()>;

/// Every MCP tool requires it, whatever its policy says (ADR-025).
pub const CONNECTOR_USE: &str = "connector:use";

#[derive(Debug, Clone)]
pub struct McpToolsetOptions {
    /// Short slug used to namespace tool names, e.g. "docs" → "mcp_docs_search".
    pub prefix: String,
    pub default_risk: ToolRisk,
    pub default_permissions: Vec<String>,
    /// Masks the server's token in what it declares and its tools return.
    pub redactor: Redactor,
}

impl McpToolsetOptions {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            default_risk: ToolRisk::Write,
            default_permissions: vec![CONNECTOR_USE.into()],
            redactor: Redactor::default(),
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
        let result = self
            .client
            .call_tool(params)
            .await
            .map_err(|error| call_failed(&error, &self.redactor))?;
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
            citations: Vec::new(),
        })
    }
}

/// The tools of a connected MCP server, with the conservative defaults.
pub async fn create_mcp_toolset(
    client: Arc<McpClient>,
    options: &McpToolsetOptions,
) -> anyhow::Result<Vec<ToolRef>> {
    let declared = declared_tools(&client, &options.prefix, &options.redactor).await?;
    let mut result: Vec<ToolRef> = Vec::with_capacity(declared.len());
    for tool in declared {
        let description = format!("[Serveur MCP « {} »] {}", options.prefix, tool.description);
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
        }));
    }
    Ok(result)
}
