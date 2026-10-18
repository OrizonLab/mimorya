//! Typed tools: one input definition gives the schema shown to the model and
//! the strict validation applied before anything runs.

use async_trait::async_trait;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::{ToolContext, ToolInfo, ToolResult, TypedTool, typed_tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SearchInput {
    /// Texte recherché.
    #[validate(length(min = 1, max = 50))]
    query: String,
    #[validate(range(min = 1, max = 20))]
    #[serde(default = "default_limit")]
    max_results: u32,
}

fn default_limit() -> u32 {
    10
}

struct Search;

#[async_trait]
impl TypedTool for Search {
    type Input = SearchInput;

    fn info(&self) -> ToolInfo {
        ToolInfo::new(
            "jobs_search",
            "Recherche",
            "Recherche des fiches.",
            ToolRisk::Read,
        )
        .requires(["jobs:read"])
    }

    fn describe_action(&self, input: &SearchInput) -> Option<String> {
        Some(format!("Rechercher « {} »", input.query))
    }

    async fn run(&self, input: SearchInput, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::ok(json!({ "query": input.query })))
    }
}

#[test]
fn derives_a_strict_inlined_schema() {
    let tool = typed_tool(Search);
    let schema = tool.input_schema();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["query"]["maxLength"], 50);
    assert_eq!(schema["required"], json!(["query"]));
    assert!(schema.get("$schema").is_none());
}

#[test]
fn normalises_valid_inputs_and_reports_issues_in_french() {
    let tool = typed_tool(Search);
    assert_eq!(
        tool.parse_input(&json!({ "query": "pompe" })).unwrap(),
        json!({ "query": "pompe", "maxResults": 10 })
    );
    let issues = tool
        .parse_input(&json!({ "query": "", "maxResults": 50 }))
        .unwrap_err();
    assert_eq!(
        issues,
        vec![
            "maxResults : valeur attendue entre 1 et 20".to_string(),
            "query : longueur attendue entre 1 et 50".to_string(),
        ]
    );
    let unknown = tool
        .parse_input(&json!({ "query": "x", "admin": true }))
        .unwrap_err();
    assert!(unknown[0].contains("structure invalide"));
    assert_eq!(
        tool.describe_action(&json!({ "query": "pompe", "maxResults": 3 }))
            .as_deref(),
        Some("Rechercher « pompe »")
    );
}
