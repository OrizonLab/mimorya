//! Reaching an MCP server (ADR-025): Streamable HTTP with the instance's
//! token, only at an address a company may use; stdio only when the operator
//! allows it.

mod support;

use mimorya_connectors::ConnectorRuntime;
use mimorya_connectors::mcp::{NetworkPolicy, http_client};
use mimorya_connectors::testing::serve_demo_docs_http;
use serde_json::json;
use support::mcp::{PRIVATE, http, port_of, start_mcp as start};

const TOKEN: &str = "jeton-de-test-0123456789";

fn names(runtime: &ConnectorRuntime) -> Vec<String> {
    runtime
        .tools
        .iter()
        .map(|t| t.info().name.clone())
        .collect()
}

#[tokio::test]
async fn connects_over_http_with_the_instance_token() {
    let url = serve_demo_docs_http(Some(TOKEN)).await;
    let runtime = start(http(&url), Some(TOKEN), PRIVATE).await.unwrap();
    assert_eq!(names(&runtime), ["mcp_docs_search_procedures"]);
    runtime.closer.unwrap().close().await;
    // Without the token, the server refuses.
    assert!(start(http(&url), None, PRIVATE).await.is_err());
}

#[tokio::test]
async fn never_reaches_a_private_address_for_a_company() {
    let url = serve_demo_docs_http(None).await;
    let saas = NetworkPolicy::default();
    // Literal private address, plain http.
    let refused = start(http(&url), None, saas).await.err().unwrap();
    assert!(format!("{refused:#}").contains("https"), "{refused:#}");
    // A name resolving to a private address, over https.
    let port = port_of(&url);
    let named = format!("https://localhost:{port}/mcp");
    assert!(start(http(&named), None, saas).await.is_err());
    let literal = format!("https://127.0.0.1:{port}/mcp");
    let refused = start(http(&literal), None, saas).await.err().unwrap();
    assert!(format!("{refused:#}").contains("privé"), "{refused:#}");
}

#[tokio::test]
async fn starts_a_stdio_server_only_when_the_operator_allows_it() {
    let config = json!({ "transport": "stdio", "prefix": "docs", "command": env!("CARGO_BIN_EXE_mcp-demo-docs") });
    let refused = start(config.clone(), None, NetworkPolicy::default())
        .await
        .err()
        .unwrap();
    assert!(format!("{refused:#}").contains("stdio"));
    let allowed = NetworkPolicy {
        allow_private_network: false,
        allow_stdio: true,
    };
    let runtime = start(config, None, allowed).await.unwrap();
    assert_eq!(names(&runtime), ["mcp_docs_search_procedures"]);
    runtime.closer.unwrap().close().await;
}

#[tokio::test]
async fn refuses_unknown_configuration_fields() {
    let config = json!({ "transport": "http", "prefix": "docs", "url": "https://mcp.example.com", "headers": {} });
    assert!(start(config, None, PRIVATE).await.is_err());
}

#[tokio::test]
async fn never_follows_a_redirect() {
    let target = serve_demo_docs_http(None).await;
    let router = axum::Router::new().route(
        "/mcp",
        axum::routing::any(move || {
            let target = target.clone();
            async move { axum::response::Redirect::temporary(&target) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let redirecting = format!("http://{address}/mcp");
    assert!(start(http(&redirecting), None, PRIVATE).await.is_err());
}

#[tokio::test]
async fn the_client_resolves_names_to_public_addresses_only() {
    let url = serve_demo_docs_http(None).await;
    let port = port_of(&url);
    let client = http_client(NetworkPolicy::default()).unwrap();
    let error = client
        .get(format!("https://localhost:{port}/mcp"))
        .send()
        .await
        .unwrap_err();
    let chain: Vec<String> =
        std::iter::successors(Some(&error as &dyn std::error::Error), |e| e.source())
            .map(ToString::to_string)
            .collect();
    assert!(
        chain.join(" / ").contains("aucune adresse publique"),
        "{chain:?}"
    );
}
