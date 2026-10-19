//! MCP bridge: exposes the tools of an MCP server as platform tools.
//!
//! MCP servers know nothing about our users, so the bridge is conservative:
//! every MCP tool defaults to risk `write` (human validation required) and to
//! the `connector:use` permission, until an administrator maps it explicitly
//! (connector `tool_policies`). Results are untrusted data like any tool output.
//!
//! What a server declares is bounded (tool count, schema size, texts), and two
//! of its tools may not share an identifier: the server is refused rather than
//! breaking the registry of the whole company.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::bail;

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{Tool, ToolContext, ToolInfo, ToolRef, ToolResult};
use rmcp::RoleClient;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use serde_json::{Value, json};

use super::failure::call_failed;
use super::redact::Redactor;

pub type McpClient = RunningService<RoleClient, ()>;

/// Every MCP tool requires it, whatever its policy says (ADR-025).
pub const CONNECTOR_USE: &str = "connector:use";
const MAX_TOOL_NAME: usize = 64;
pub const MAX_TOOLS: usize = 200;
const MAX_SCHEMA_BYTES: usize = 32 * 1024;
/// What a server says about its tools reaches the model: kept short.
const MAX_TITLE_CHARS: usize = 120;
const MAX_DESCRIPTION_CHARS: usize = 1000;

fn bounded(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut kept: String = text.chars().take(max - 1).collect();
    kept.push('…');
    kept
}

#[derive(Debug, Clone)]
pub struct McpToolsetOptions {
    /// Short slug used to namespace tool names, e.g. "docs" → "mcp_docs_search".
    pub prefix: String,
    pub default_risk: ToolRisk,
    pub default_permissions: Vec<String>,
    /// Masks the server's token in what its tools return.
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

/// The start shared by the names of a server's tools, e.g. "mcp_docs_".
pub fn tool_namespace(prefix: &str) -> String {
    format!("mcp_{}_", slug(prefix))
}

/// Namespaced tool name within provider limits.
pub fn mcp_tool_name(prefix: &str, tool: &str) -> String {
    format!("{}{}", tool_namespace(prefix), slug(tool))
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
    let tools = client
        .list_all_tools()
        .await
        .map_err(|error| call_failed(&error, &options.redactor))?;
    if tools.len() > MAX_TOOLS {
        bail!(
            "Le serveur déclare {} outils : {MAX_TOOLS} au plus",
            tools.len()
        );
    }
    let mut result: Vec<ToolRef> = Vec::with_capacity(tools.len());
    let mut remote_names: HashMap<String, String> = HashMap::new();
    for tool in tools {
        let name = mcp_tool_name(&options.prefix, &tool.name);
        if let Some(other) = remote_names.insert(name.clone(), tool.name.to_string()) {
            bail!(
                "Les outils « {} » et « {} » du serveur donnent le même identifiant ({name}) : \
                 renommez l’un d’eux sur le serveur",
                bounded(&other, MAX_TITLE_CHARS),
                bounded(&tool.name, MAX_TITLE_CHARS)
            );
        }
        let input_schema = Value::Object((*tool.input_schema).clone());
        if input_schema.to_string().len() > MAX_SCHEMA_BYTES {
            bail!(
                "Le schéma de l’outil « {} » dépasse {} Kio",
                bounded(&tool.name, MAX_TITLE_CHARS),
                MAX_SCHEMA_BYTES / 1024
            );
        }
        let title = tool
            .title
            .clone()
            .or_else(|| tool.annotations.as_ref().and_then(|a| a.title.clone()))
            .unwrap_or_else(|| tool.name.to_string());
        let description = format!(
            "[Serveur MCP « {} »] {}",
            options.prefix,
            bounded(
                tool.description.as_deref().unwrap_or(&tool.name),
                MAX_DESCRIPTION_CHARS
            )
        );
        let info = ToolInfo::try_new(
            &name,
            &bounded(&title, MAX_TITLE_CHARS),
            &description,
            options.default_risk,
        )
        .map_err(anyhow::Error::msg)?
        .requires(options.default_permissions.clone());
        result.push(Arc::new(McpTool {
            info,
            remote_name: tool.name.to_string(),
            prefix: options.prefix.clone(),
            input_schema,
            client: client.clone(),
            redactor: options.redactor.clone(),
        }));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_what_a_server_says_by_characters() {
        assert_eq!(bounded("abc", 3), "abc");
        assert_eq!(bounded("abcd", 3), "ab…");
        // Multibyte characters count once and are never cut.
        assert_eq!(bounded("éèàù", 4), "éèàù");
        assert_eq!(bounded("éèàùç", 4), "éèà…");
        assert_eq!(bounded(&"x".repeat(5000), 1000).chars().count(), 1000);
    }

    #[test]
    fn a_namespace_starts_every_tool_name_of_its_server() {
        assert_eq!(tool_namespace("Docs"), "mcp_docs_");
        assert!(mcp_tool_name("docs", "get-item").starts_with(&tool_namespace("docs")));
    }
}
