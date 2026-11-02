//! What keeps a session whose call failed (ADR-164): a call past its delay
//! is cancelled, and the server pinged, one ping for failures that come
//! together; a server that answers the ping keeps its session, even
//! refusing it as a method it does not know or with a 429. A refusal of
//! the server's own is an answer: no ping follows it.

mod support;

use std::time::Duration;

use mimorya_connectors::ConnectorRuntime;
use mimorya_connectors::outbound::TOO_SLOW;
use mimorya_connectors::testing::{PING_DELAY, Ping, SLOW_CALL, Seen, serve_slow_mcp_http};
use serde_json::json;
use support::mcp::{PRIVATE, call_as_reader, http, start_mcp_calling_within};

/// What a call may take here: the ping that follows a failure has as long.
const CALL: Duration = Duration::from_secs(1);

async fn start(ping: Ping) -> (ConnectorRuntime, std::sync::Arc<Seen>) {
    let (url, seen) = serve_slow_mcp_http(ping).await;
    let runtime = start_mcp_calling_within(http(&url), PRIVATE, CALL)
        .await
        .unwrap();
    (runtime, seen)
}

/// Calls the slow tool, which fails past its delay.
async fn too_slow(runtime: &ConnectorRuntime) {
    let result = call_as_reader(runtime, "mcp_docs_get_item", json!({})).await;
    let said = result.content.to_string();
    assert!(result.is_error && said.contains(TOO_SLOW), "{said}");
}

/// Whether `count` reaches `expected` within five seconds.
async fn reaches(count: impl Fn() -> usize, expected: usize) -> bool {
    let reached = async {
        while count() < expected {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), reached)
        .await
        .is_ok()
}

#[tokio::test]
async fn a_slow_tool_keeps_the_session_of_a_server_that_answers_its_ping() {
    assert!(PING_DELAY < CALL && CALL * 2 < SLOW_CALL);
    for ping in [Ping::Answered, Ping::Unknown, Ping::RateLimited] {
        let (runtime, seen) = start(ping).await;
        too_slow(&runtime).await;
        assert!(reaches(|| seen.pings(), 1).await, "{ping:?}: no ping");
        // The ping's whole bound over: the session is kept.
        tokio::time::sleep(CALL).await;
        assert!(!runtime.is_closed(), "{ping:?}");
        runtime.closer.unwrap().close().await;
    }
}

#[tokio::test]
async fn calls_past_their_delay_are_cancelled_and_failing_together_send_one_ping() {
    let (runtime, seen) = start(Ping::Answered).await;
    // Three calls that fail together, while the first ping is under way.
    tokio::join!(too_slow(&runtime), too_slow(&runtime), too_slow(&runtime));
    assert!(reaches(|| seen.cancelled(), 3).await, "{seen:?}");
    tokio::time::sleep(CALL).await;
    assert_eq!(seen.pings(), 1);
    assert!(!runtime.is_closed());
    runtime.closer.unwrap().close().await;
}

#[tokio::test]
async fn a_refusal_of_the_server_s_own_starts_no_ping() {
    let (runtime, seen) = start(Ping::Answered).await;
    let result = call_as_reader(&runtime, "mcp_docs_refused_item", json!({})).await;
    let said = result.content.to_string();
    assert!(result.is_error && said.contains("Élément refusé"), "{said}");
    tokio::time::sleep(CALL).await;
    assert_eq!(seen.pings(), 0);
    assert!(!runtime.is_closed());
    runtime.closer.unwrap().close().await;
}
