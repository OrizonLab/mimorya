//! MCP bridge against the demo documentation server (in-memory transport).

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::{ToolCallEvaluation, ToolOverride, ToolRegistry, wrap_untrusted};
use mimorya_connectors::mcp::{
    DemoDocsServer, McpClient, McpToolsetOptions, create_mcp_toolset, mcp_tool_name,
};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use rmcp::ServiceExt;
use serde_json::json;
use support::{executor, executor_for};

async fn client() -> Arc<McpClient> {
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let server = DemoDocsServer::default().serve(server_io).await.unwrap();
        server.waiting().await.unwrap();
    });
    Arc::new(().serve(client_io).await.unwrap())
}

fn call(name: &str, query: &str) -> LlmToolCall {
    LlmToolCall {
        id: "1".into(),
        name: name.into(),
        input: json!({ "query": query }),
    }
}

#[test]
fn namespaces_tool_names_within_provider_limits() {
    assert_eq!(
        mcp_tool_name("Docs Néo", "search-procedures"),
        "mcp_docs_n_o_search_procedures"
    );
    assert!(mcp_tool_name(&"x".repeat(80), "y").len() <= 64);
}

#[tokio::test]
async fn requires_human_validation_for_unmapped_mcp_tools() {
    let tools = create_mcp_toolset(client().await, &McpToolsetOptions::new("docs"))
        .await
        .unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.info().name.as_str()).collect();
    assert_eq!(names, ["mcp_docs_search_procedures"]);
    let schema = tools[0].input_schema();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["required"], json!(["query"]));
    assert_eq!(tools[0].info().risk, ToolRisk::Write);

    let ports = InMemoryPorts::new(user::LUCAS, uuid::Uuid::new_v4());
    let evaluation = executor(tools, user::LUCAS, &ports)
        .evaluate(&call("mcp_docs_search_procedures", "circulateur"));
    assert_eq!(evaluation.kind(), "require_approval");
}

#[tokio::test]
async fn runs_tools_mapped_by_the_administrator_with_the_mapped_permission() {
    let tools = create_mcp_toolset(client().await, &McpToolsetOptions::new("docs"))
        .await
        .unwrap();
    let overrides = HashMap::from([(
        "mcp_docs_search_procedures".to_string(),
        ToolOverride {
            risk: Some(ToolRisk::Read),
            permissions: Some(vec!["intervention:read".into()]),
            ..Default::default()
        },
    )]);
    let registry = ToolRegistry::new(tools, &overrides).unwrap();
    let ports = InMemoryPorts::new(user::LUCAS, uuid::Uuid::new_v4());

    let lucas = executor_for(registry.clone(), user::LUCAS, &ports);
    let ToolCallEvaluation::Allow { tool, input } =
        lucas.evaluate(&call("mcp_docs_search_procedures", "circulateur"))
    else {
        panic!("expected the mapped tool to run directly");
    };
    let (result, _) = lucas.execute(&tool, input).await;
    assert!(!result.is_error);
    assert!(wrap_untrusted("mcp", &result.content).contains("PROC-12"));

    // The administrator mapped it to intervention:read: Paul (IT admin) cannot use it.
    let paul = executor_for(registry, user::PAUL, &ports);
    assert_eq!(
        paul.evaluate(&call("mcp_docs_search_procedures", "x"))
            .kind(),
        "deny"
    );
}

#[tokio::test]
async fn can_be_disabled_by_the_administrator() {
    let tools = create_mcp_toolset(client().await, &McpToolsetOptions::new("docs"))
        .await
        .unwrap();
    let overrides = HashMap::from([(
        "mcp_docs_search_procedures".to_string(),
        ToolOverride {
            enabled: Some(false),
            ..Default::default()
        },
    )]);
    assert!(
        ToolRegistry::new(tools, &overrides)
            .unwrap()
            .all()
            .is_empty()
    );
}
