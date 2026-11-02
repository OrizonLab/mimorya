//! What is read of an MCP server's answers (ADR-161): an answer that never
//! ends, JSON, event stream or refusal, is read up to its bound and no
//! further. The call fails at once, saying why, rather than when its delay
//! runs out with all it read held; the session goes on. A start whose
//! answer never ends says why it failed, and each event of the session's
//! stream is bounded as an answer.

mod support;

use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt;
use mimorya_connectors::ConnectorRuntime;
use mimorya_connectors::mcp::{BoundedHttpClient, MAX_RESPONSE_BYTES, NOT_MCP, TOO_LONG};
use mimorya_connectors::outbound::http_client;
use mimorya_connectors::testing::{Quirk, serve_mcp_http, serve_one_event};
use rmcp::transport::streamable_http_client::StreamableHttpClient;
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

/// What a failed start, from a server with `quirk`, says.
async fn failed_start(quirk: Quirk) -> String {
    let error = start(quirk).await.err().expect("the start fails");
    format!("{error:#}")
}

/// Said by the platform, as is: not as an error of the server's.
fn said_as_is() -> String {
    json!({ "erreur": TOO_LONG }).to_string()
}

#[tokio::test]
async fn json_that_never_ends_is_not_read_beyond_its_bound() {
    assert_eq!(call(Quirk::EndlessJsonOnCall).await, said_as_is());
}

#[tokio::test]
async fn an_event_that_never_ends_is_not_read_beyond_its_bound() {
    assert_eq!(call(Quirk::EndlessEventOnCall).await, said_as_is());
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
    assert_eq!(failed_start(Quirk::EndlessJsonOnList).await, TOO_LONG);
}

#[tokio::test]
async fn a_start_answered_without_end_says_why_it_failed() {
    // As JSON, or as an event stream: the MCP library tells neither.
    assert_eq!(failed_start(Quirk::EndlessJsonOnInitialize).await, TOO_LONG);
    assert_eq!(
        failed_start(Quirk::EndlessEventOnInitialize).await,
        TOO_LONG
    );
}

#[tokio::test]
async fn an_event_of_the_session_s_stream_is_bounded_as_an_answer() {
    let client = BoundedHttpClient::new(http_client(PRIVATE).unwrap());
    // Whatever bound per event the transport asks for: the MCP library's
    // default is 16 MiB.
    let asked = 16 * 1024 * 1024;
    for (size, read) in [
        (MAX_RESPONSE_BYTES - 1024, true),
        (MAX_RESPONSE_BYTES + 1, false),
    ] {
        let url = serve_one_event(size).await;
        let stream = client.get_stream_with_max_sse_event_size(
            url.into(),
            None,
            None,
            None,
            HashMap::new(),
            asked,
        );
        let mut events = stream.await.expect("a stream");
        let first = events.next().await.expect("an event or an error");
        assert_eq!(first.is_ok(), read, "{size} bytes");
    }
}
