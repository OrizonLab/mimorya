//! What an MCP server declares about its tools, read with care. At most
//! `MAX_TOOLS` tools and `MAX_PAGES` pages are read; the token is masked in every title,
//! description and schema before anything is shortened, and a tool name may
//! not quote it (an identifier cannot be masked). Names, schemas and the
//! declarations as a whole are bounded, and two tools may not share an
//! identifier once namespaced. A server breaking a rule is refused whole:
//! its connector alone is then unavailable.

use std::collections::HashMap;

use anyhow::bail;
use rmcp::model::{PaginatedRequestParams, Tool};
use serde_json::Value;

use super::bridge::McpClient;
use super::failure::call_failed;
use super::redact::Redactor;

pub const MAX_TOOLS: usize = 200;
/// Pages may be short, not endless.
const MAX_PAGES: usize = 50;
const MAX_TOOL_NAME: usize = 64;
/// The length the MCP specification recommends for a tool name.
const MAX_REMOTE_NAME_CHARS: usize = 128;
const MAX_TITLE_CHARS: usize = 120;
/// A tool's description, as a server declares it or an administrator
/// replaces it.
pub const MAX_DESCRIPTION_CHARS: usize = 1000;
const MAX_SCHEMA_BYTES: usize = 32 * 1024;
/// What one server's declarations may add to every model call.
const MAX_DECLARATIONS_BYTES: usize = 256 * 1024;

/// A tool as the platform shows it: namespaced, masked, bounded.
pub struct Declared {
    pub name: String,
    /// The name the server answers to.
    pub remote_name: String,
    pub title: String,
    pub description: String,
    pub input_schema: Value,
}

pub fn bounded(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut kept: String = text.chars().take(max - 1).collect();
    kept.push('…');
    kept
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

/// The server's tools, read page by page and checked.
pub async fn declared_tools(
    client: &McpClient,
    prefix: &str,
    redactor: &Redactor,
) -> anyhow::Result<Vec<Declared>> {
    let mut tools = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PAGES {
        let params = PaginatedRequestParams::default().with_cursor(cursor);
        let page = client
            .list_tools(Some(params))
            .await
            .map_err(|error| call_failed(&error, redactor))?;
        tools.extend(page.tools);
        if tools.len() > MAX_TOOLS {
            bail!("Le serveur déclare plus de {MAX_TOOLS} outils");
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return checked(tools, prefix, redactor);
        }
    }
    bail!("Le serveur répartit ses outils sur plus de {MAX_PAGES} pages")
}

/// The declarations the platform keeps, or why the server is refused.
pub fn checked(
    tools: Vec<Tool>,
    prefix: &str,
    redactor: &Redactor,
) -> anyhow::Result<Vec<Declared>> {
    let mut declared = Vec::with_capacity(tools.len());
    let mut remote_names: HashMap<String, String> = HashMap::new();
    let mut total_bytes = 0;
    for tool in tools {
        let remote_name = tool.name.to_string();
        if redactor.reveals(&remote_name) {
            bail!("Un nom d’outil du serveur recopie le jeton d’accès");
        }
        if remote_name.chars().count() > MAX_REMOTE_NAME_CHARS {
            bail!(
                "Le nom de l’outil « {} » dépasse {MAX_REMOTE_NAME_CHARS} caractères",
                bounded(&remote_name, MAX_TITLE_CHARS)
            );
        }
        let name = mcp_tool_name(prefix, &remote_name);
        if let Some(other) = remote_names.insert(name.clone(), remote_name.clone()) {
            bail!(
                "Les outils « {other} » et « {remote_name} » du serveur donnent le même \
                 identifiant ({name}) : renommez l’un d’eux sur le serveur"
            );
        }
        let input_schema = redactor.value(Value::Object((*tool.input_schema).clone()));
        let schema_bytes = input_schema.to_string().len();
        if schema_bytes > MAX_SCHEMA_BYTES {
            bail!(
                "Le schéma de l’outil « {remote_name} » dépasse {} Kio",
                MAX_SCHEMA_BYTES / 1024
            );
        }
        let title = tool
            .title
            .clone()
            .or_else(|| tool.annotations.as_ref().and_then(|a| a.title.clone()))
            .unwrap_or_else(|| remote_name.clone());
        let title = bounded(&redactor.text(&title), MAX_TITLE_CHARS);
        let description = tool.description.as_deref().unwrap_or(&remote_name);
        let description = bounded(&redactor.text(description), MAX_DESCRIPTION_CHARS);
        total_bytes += schema_bytes + title.len() + description.len();
        if total_bytes > MAX_DECLARATIONS_BYTES {
            bail!(
                "Les déclarations d’outils du serveur dépassent {} Kio",
                MAX_DECLARATIONS_BYTES / 1024
            );
        }
        declared.push(Declared {
            name,
            remote_name,
            title,
            description,
            input_schema,
        });
    }
    Ok(declared)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;

    const TOKEN: &str = "jeton-0123456789";

    fn tool(name: &str, description: &str, schema: Value) -> Tool {
        let Value::Object(schema) = schema else {
            panic!("an object schema");
        };
        Tool::new(name.to_string(), description.to_string(), Arc::new(schema))
    }

    fn refusal(tools: Vec<Tool>) -> String {
        match checked(tools, "docs", &Redactor::new(Some(TOKEN))) {
            Err(error) => error.to_string(),
            Ok(_) => panic!("expected the server to be refused"),
        }
    }

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

    #[test]
    fn masks_the_token_before_shortening_anything() {
        let schema = json!({ "type": "object", "description": format!("Bearer {TOKEN}") });
        // The token straddles the description limit.
        let description = format!("{}{TOKEN}", "x".repeat(MAX_DESCRIPTION_CHARS - 10));
        let mut quoting = tool("search", &description, schema);
        quoting.title = Some(format!("Recherche {TOKEN}"));
        let declared = checked(vec![quoting], "docs", &Redactor::new(Some(TOKEN))).unwrap();
        let shown = format!(
            "{} {} {}",
            declared[0].title, declared[0].description, declared[0].input_schema
        );
        assert!(!shown.contains(TOKEN), "{shown}");
        assert!(!shown.contains("jeton-0"), "{shown}");
        assert!(declared[0].description.ends_with("x***"), "{shown}");
        assert!(shown.contains("Bearer ***"), "{shown}");
        assert_eq!(declared[0].remote_name, "search");
    }

    #[test]
    fn refuses_names_quoting_the_token_too_long_or_colliding() {
        let empty = || json!({ "type": "object" });
        let message = refusal(vec![tool(&format!("debug-{TOKEN}"), "x", empty())]);
        assert!(!message.contains(TOKEN), "{message}");
        assert!(message.contains("recopie le jeton"), "{message}");
        let message = refusal(vec![tool(&"n".repeat(129), "x", empty())]);
        assert!(message.contains("dépasse 128 caractères"), "{message}");
        let message = refusal(vec![
            tool("get-item", "x", empty()),
            tool("get_item", "x", empty()),
        ]);
        assert!(
            message.contains("« get-item » et « get_item »"),
            "{message}"
        );
        assert!(message.contains("(mcp_docs_get_item)"), "{message}");
    }

    #[test]
    fn bounds_each_schema_and_the_declarations_as_a_whole() {
        let sized = |name: &str, bytes: usize| {
            tool(
                name,
                "x",
                json!({ "type": "object", "description": "x".repeat(bytes) }),
            )
        };
        let message = refusal(vec![sized("big", MAX_SCHEMA_BYTES)]);
        assert!(message.contains("dépasse 32 Kio"), "{message}");
        let many: Vec<Tool> = (0..9).map(|i| sized(&format!("t{i}"), 30 * 1024)).collect();
        let message = refusal(many);
        assert!(message.contains("dépassent 256 Kio"), "{message}");
        let fine: Vec<Tool> = (0..8).map(|i| sized(&format!("t{i}"), 30 * 1024)).collect();
        assert_eq!(
            checked(fine, "docs", &Redactor::default()).unwrap().len(),
            8
        );
    }
}
