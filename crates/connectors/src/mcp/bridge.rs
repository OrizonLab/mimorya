//! MCP bridge: exposes the tools of an MCP server as platform tools.
//!
//! MCP servers know nothing about our users, so the bridge is conservative:
//! every MCP tool defaults to risk `write` (human validation required) and to
//! the `connector:use` permission, until an administrator maps it explicitly
//! (connector `tool_policies`). Results are untrusted data like any tool output.

use std::sync::Arc;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Tool, ToolContext, ToolInfo, ToolRef, ToolResult};
use rmcp::RoleClient;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use serde_json::{Value, json};

pub type McpClient = RunningService<RoleClient, ()>;

const MAX_TOOL_NAME: usize = 64;

#[derive(Debug, Clone)]
pub struct McpToolsetOptions {
    /// Short slug used to namespace tool names, e.g. "docs" → "mcp_docs_search".
    pub prefix: String,
    pub default_risk: ToolRisk,
    pub default_permissions: Vec<String>,
}

impl McpToolsetOptions {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            default_risk: ToolRisk::Write,
            default_permissions: vec!["connector:use".into()],
        }
    }
}

fn slug(value: &str) -> String {
    let mut slug = String::new();
    for c in value.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    slug.trim_matches('_').to_string()
}

/// Namespaced tool name within provider limits.
pub fn mcp_tool_name(prefix: &str, tool: &str) -> String {
    format!("mcp_{}_{}", slug(prefix), slug(tool))
        .chars()
        .take(MAX_TOOL_NAME)
        .collect()
}

struct McpTool {
    info: ToolInfo,
    remote_name: String,
    prefix: String,
    input_schema: Value,
    client: Arc<McpClient>,
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
        let result = self.client.call_tool(params).await?;
        let text: Vec<String> = result
            .content
            .iter()
            .map(|part| match part.as_text() {
                Some(text) => text.text.clone(),
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
            content: result
                .structured_content
                .unwrap_or_else(|| json!(text.join("\n"))),
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
    let tools = client.list_all_tools().await?;
    let mut result: Vec<ToolRef> = Vec::with_capacity(tools.len());
    for tool in tools {
        let title = tool
            .title
            .clone()
            .or_else(|| tool.annotations.as_ref().and_then(|a| a.title.clone()))
            .unwrap_or_else(|| tool.name.to_string());
        let description = format!(
            "[Serveur MCP « {} »] {}",
            options.prefix,
            tool.description.as_deref().unwrap_or(&tool.name)
        );
        let info = ToolInfo::try_new(
            &mcp_tool_name(&options.prefix, &tool.name),
            &title,
            &description,
            options.default_risk,
        )
        .map_err(anyhow::Error::msg)?
        .requires(options.default_permissions.clone());
        result.push(Arc::new(McpTool {
            info,
            remote_name: tool.name.to_string(),
            prefix: options.prefix.clone(),
            input_schema: Value::Object((*tool.input_schema).clone()),
            client: client.clone(),
        }));
    }
    Ok(result)
}
