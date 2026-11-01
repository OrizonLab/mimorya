//! What is read of an MCP server's answers (ADR-161): an answer that never
//! ends, JSON, event stream or refusal, is read up to its bound and no
//! further. The call fails at once, saying why, rather than when its delay
//! runs out with all it read held; the session goes on, and a server whose
//! tool list never ends is not started.

mod support;

use std::time::Duration;

use mimorya_connectors::ConnectorRuntime;
use mimorya_connectors::mcp::{NOT_MCP, TOO_LONG};
use mimorya_connectors::testing::{Quirk, serve_mcp_http};
use serde_json::json;
use support::mcp::{PRIVATE, call_as_reader, http, start_mcp_within};

const TOOL: &str = "mcp_docs_get_item";

/// Far less than any delay of a start or a call: a bounded answer is read
/// at once.
const AT_ONCE: Duration = Duration::from_secs(10);

async fn start(quirk: Quirk) -> anyhow::Result<ConnectorRuntime> {
    let url = serve_mcp_http(None, quirk).await;
    let start = start_mcp_within(http(&url), None, PRIVATE, Duration::from_secs(60));
    tokio::time::timeout(AT_ONCE, start)
        .await
        .expect("the start ends at once")
}

/// What a call to the tool says, from a server with `quirk`; the session
/// still open after it.
async fn call(quirk: Quirk) -> String {
    let runtime = start(quirk).await.expect("the server starts");
    let result = tokio::time::timeout(AT_ONCE, call_as_reader(&runtime, TOOL, json!({})))
        .await
        .expect("the call ends at once");
    assert!(result.is_error, "{}", result.content);
    let closer = runtime.closer.expect("a session");
    assert!(!closer.is_closed(), "the session goes on");
    closer.close().await;
    result.content.to_string()
}

#[tokio::test]
async fn json_that_never_ends_is_not_read_beyond_its_bound() {
    let said = call(Quirk::EndlessJsonOnCall).await;
    assert!(said.contains(TOO_LONG), "{said}");
}

#[tokio::test]
async fn an_event_that_never_ends_is_not_read_beyond_its_bound() {
    let said = call(Quirk::EndlessEventOnCall).await;
    assert!(said.contains(TOO_LONG), "{said}");
}

#[tokio::test]
async fn a_refusal_that_never_ends_is_told_from_its_beginning() {
    let said = call(Quirk::EndlessErrorOnCall).await;
    assert!(said.contains(NOT_MCP), "{said}");
    assert!(said.contains("HTTP 500"), "{said}");
    assert!(said.contains("Erreur interne : xxx"), "{said}");
}

#[tokio::test]
async fn a_tool_list_that_never_ends_stops_the_start() {
    let error = start(Quirk::EndlessJsonOnList)
        .await
        .err()
        .expect("the start fails");
    let said = format!("{error:#}");
    assert!(said.contains(TOO_LONG), "{said}");
}
