//! How an MCP server can fail (ADR-025): each failure is explained in
//! French, never quotes the token, and what a server declares is bounded.

mod support;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::{ToolCallEvaluation, ToolOverride, ToolRegistry};
use mimorya_connectors::mcp::{
    ACCESS_DENIED, MASK, NOT_MCP, NOT_PUBLIC, NetworkPolicy, TOO_SLOW, UNREACHABLE,
};
use mimorya_connectors::testing::{Quirk, serve_mcp_http, serve_mute};
use mimorya_connectors::{ConnectorRuntime, InstanceContext, connector_type};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use serde_json::json;
use support::executor_for;

const TOKEN: &str = "jeton-de-test-0123456789";

const PRIVATE: NetworkPolicy = NetworkPolicy {
    allow_private_network: true,
    allow_stdio: false,
};

async fn start_with(
    url: &str,
    secret: Option<&str>,
    network: NetworkPolicy,
    start_timeout: Duration,
) -> anyhow::Result<ConnectorRuntime> {
    let config = json!({ "transport": "http", "prefix": "docs", "url": url });
    let context = InstanceContext {
        secret,
        network,
        start_timeout,
    };
    connector_type("mcp")
        .unwrap()
        .instantiate(&config, context)
        .await
}

/// The message an administrator would read.
async fn failure(url: &str, secret: Option<&str>, network: NetworkPolicy) -> String {
    let error = start_with(url, secret, network, Duration::from_secs(10))
        .await
        .err()
        .expect("the start fails");
    format!("{error:#}")
}

#[tokio::test]
async fn refuses_a_server_whose_tools_share_an_identifier() {
    let url = serve_mcp_http(None, Quirk::CollidingTools).await;
    let message = failure(&url, None, PRIVATE).await;
    assert!(
        message.contains("« get-item » et « get_item »"),
        "{message}"
    );
    assert!(message.contains("mcp_docs_get_item"), "{message}");
}

#[tokio::test]
async fn bounds_what_a_server_declares() {
    let url = serve_mcp_http(None, Quirk::TooManyTools).await;
    let message = failure(&url, None, PRIVATE).await;
    assert!(message.contains("plus de 200 outils"), "{message}");
    let url = serve_mcp_http(None, Quirk::EndlessPages).await;
    let message = failure(&url, None, PRIVATE).await;
    assert!(message.contains("plus de 50 pages"), "{message}");
    let url = serve_mcp_http(None, Quirk::EndlessEmptyPages).await;
    let message = failure(&url, None, PRIVATE).await;
    assert!(message.contains("plus de 50 pages"), "{message}");
    let url = serve_mcp_http(None, Quirk::HugeSchema).await;
    let message = failure(&url, None, PRIVATE).await;
    assert!(message.contains("dépasse 32 Kio"), "{message}");
    let url = serve_mcp_http(None, Quirk::None).await;
    assert!(
        start_with(&url, None, PRIVATE, Duration::from_secs(10))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn never_quotes_the_token_a_server_sends_back_at_start() {
    let url = serve_mcp_http(Some(TOKEN), Quirk::EchoesTokenOnStart).await;
    let message = failure(&url, Some(TOKEN), PRIVATE).await;
    assert!(!message.contains(TOKEN), "{message}");
    assert!(message.contains(&format!("Bearer {MASK}")), "{message}");
    assert!(message.starts_with(NOT_MCP), "{message}");
    // No type name of the MCP library.
    assert!(!message.contains("rmcp::"), "{message}");
}

#[tokio::test]
async fn never_quotes_the_token_a_server_sends_back_escaped() {
    let token = "jeton/de+test=0123456789";
    let url = serve_mcp_http(Some(token), Quirk::EchoesEscapedTokenOnStart).await;
    let message = failure(&url, Some(token), PRIVATE).await;
    assert!(
        message.contains(&format!(r#""message":"Bearer {MASK}""#)),
        "{message}"
    );
    assert!(!message.contains("0123456789"), "{message}");
}

#[tokio::test]
async fn never_quotes_the_token_a_server_sends_back_on_a_call() {
    let url = serve_mcp_http(Some(TOKEN), Quirk::EchoesTokenOnCall).await;
    let runtime = start_with(&url, Some(TOKEN), PRIVATE, Duration::from_secs(10))
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
    let registry = ToolRegistry::new(runtime.tools.clone(), &overrides).unwrap();
    let ports = InMemoryPorts::new(user::LUCAS, uuid::Uuid::new_v4());
    let lucas = executor_for(registry, user::LUCAS, &ports);
    let call = LlmToolCall {
        id: "1".into(),
        name: "mcp_docs_search_procedures".into(),
        input: json!({ "query": "circulateur" }),
    };
    let ToolCallEvaluation::Allow { tool, input } = lucas.evaluate(&call) else {
        panic!("expected the mapped tool to run directly");
    };
    let (result, _) = lucas.execute(&tool, input).await;
    assert!(result.is_error);
    let shown = result.content.to_string();
    assert!(!shown.contains(TOKEN), "{shown}");
    assert!(shown.contains(MASK), "{shown}");
    runtime.closer.unwrap().close().await;
}

#[tokio::test]
async fn never_shows_the_token_a_tool_result_quotes() {
    let url = serve_mcp_http(Some(TOKEN), Quirk::EchoesTokenInResults).await;
    let runtime = start_with(&url, Some(TOKEN), PRIVATE, Duration::from_secs(10))
        .await
        .unwrap();
    let read = ToolOverride {
        risk: Some(ToolRisk::Read),
        permissions: Some(vec!["intervention:read".into()]),
        ..Default::default()
    };
    let names = ["mcp_docs_echo_text", "mcp_docs_echo_structured"];
    let overrides = names
        .iter()
        .map(|name| (name.to_string(), read.clone()))
        .collect();
    let registry = ToolRegistry::new(runtime.tools.clone(), &overrides).unwrap();
    let ports = InMemoryPorts::new(user::LUCAS, uuid::Uuid::new_v4());
    let lucas = executor_for(registry, user::LUCAS, &ports);
    for name in names {
        let call = LlmToolCall {
            id: "1".into(),
            name: name.into(),
            input: json!({}),
        };
        let ToolCallEvaluation::Allow { tool, input } = lucas.evaluate(&call) else {
            panic!("expected {name} to run directly");
        };
        let (result, _) = lucas.execute(&tool, input).await;
        let shown = result.content.to_string();
        assert!(!result.is_error, "{shown}");
        assert!(!shown.contains(TOKEN), "{shown}");
        assert!(shown.contains(&format!("Bearer {MASK}")), "{shown}");
    }
    runtime.closer.unwrap().close().await;
}

#[tokio::test]
async fn never_shows_the_token_a_server_quotes_in_its_declarations() {
    let url = serve_mcp_http(Some(TOKEN), Quirk::EchoesTokenInDeclarations).await;
    let runtime = start_with(&url, Some(TOKEN), PRIVATE, Duration::from_secs(10))
        .await
        .unwrap();
    let tool = &runtime.tools[0];
    let shown = format!(
        "{} {} {}",
        tool.info().title,
        tool.info().description,
        tool.input_schema()
    );
    assert!(!shown.contains(TOKEN), "{shown}");
    let masked = format!("Bearer {MASK}");
    assert!(tool.info().title.ends_with(&masked), "{shown}");
    assert!(
        tool.info()
            .description
            .ends_with(&format!("Debug: {masked}")),
        "{shown}"
    );
    assert!(tool.input_schema().to_string().contains(&masked), "{shown}");
    runtime.closer.unwrap().close().await;
}

#[tokio::test]
async fn explains_why_a_server_cannot_be_reached() {
    let url = serve_mcp_http(Some(TOKEN), Quirk::None).await;
    let message = failure(&url, None, PRIVATE).await;
    assert!(message.starts_with(ACCESS_DENIED), "{message}");
    let message = failure(&url, Some("un-autre-jeton-0000"), PRIVATE).await;
    assert!(message.starts_with(ACCESS_DENIED), "{message}");

    let message = failure("http://127.0.0.1:9/mcp", None, PRIVATE).await;
    assert!(message.starts_with(UNREACHABLE), "{message}");
    assert!(!message.contains("rmcp::"), "{message}");

    let port = url.rsplit(':').next().unwrap().trim_end_matches("/mcp");
    let named = format!("https://localhost:{port}/mcp");
    let message = failure(&named, None, NetworkPolicy::default()).await;
    assert!(message.starts_with(NOT_PUBLIC), "{message}");
}

#[tokio::test]
async fn gives_up_on_a_mute_server_after_the_start_timeout() {
    let url = serve_mute().await;
    let started = Instant::now();
    let error = start_with(&url, None, PRIVATE, Duration::from_millis(300))
        .await
        .err()
        .unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(format!("{error:#}").starts_with(TOO_SLOW), "{error:#}");
}
