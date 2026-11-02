//! A session with an MCP server kept between runs: an HTTP server that
//! restarts and says so (404) is reached again by the same session (the
//! client opens a new one); a local server (stdio) that exits leaves a
//! session that says it is closed, for the platform to start it again. So
//! does a session whose server did not answer a ping after a failed call
//! (ADR-164): a server stopped, mute, or that refuses a session it no longer
//! knows without saying 404. A slow tool of a server that answers its ping
//! keeps its session.

mod support;

use std::path::PathBuf;
use std::time::Duration;

use mimorya_connectors::ConnectorRuntime;
use mimorya_connectors::outbound::{NetworkPolicy, TOO_SLOW};
use mimorya_connectors::testing::{
    Quirk, SLOW_CALL, serve_mcp_http, serve_mcp_http_on, serve_stoppable_demo_docs_http,
};
use serde_json::{Value, json};
use support::mcp::{PRIVATE, call_as_reader, http, port_of, start_mcp, start_mcp_calling_within};

/// What a call may take here: the ping that follows a failure has as long.
const CALL: Duration = Duration::from_secs(1);

/// Whether a search through the session's tool succeeds.
async fn searched(runtime: &ConnectorRuntime) -> bool {
    let search = json!({ "query": "circulateur" });
    let result = call_as_reader(runtime, "mcp_docs_search_procedures", search).await;
    !result.is_error
}

/// Whether the session says it is closed within ten seconds.
async fn closes(runtime: &ConnectorRuntime) -> bool {
    let closed = async {
        while !runtime.is_closed() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), closed)
        .await
        .is_ok()
}

#[tokio::test]
async fn an_http_session_reaches_its_server_again_after_a_restart() {
    let (url, stop) = serve_stoppable_demo_docs_http(0).await;
    let runtime = start_mcp(http(&url), None, PRIVATE).await.unwrap();
    assert!(searched(&runtime).await);

    // Back between two calls, without the old session: the same runtime
    // reaches it again, told so by a 404.
    stop.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (_, _running) = serve_stoppable_demo_docs_http(port_of(&url)).await;
    assert!(searched(&runtime).await);
    assert!(!runtime.is_closed());
    runtime.closer.unwrap().close().await;
}

#[tokio::test]
async fn an_http_server_that_stopped_loses_its_session() {
    let (url, stop) = serve_stoppable_demo_docs_http(0).await;
    let runtime = start_mcp(http(&url), None, PRIVATE).await.unwrap();
    stop.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    // The call fails, then the ping: started again by the next run.
    assert!(!searched(&runtime).await);
    assert!(closes(&runtime).await, "the session never said it was lost");
}

#[tokio::test]
async fn a_server_that_refuses_an_unknown_session_without_404_loses_it() {
    let (url, stop) = serve_stoppable_demo_docs_http(0).await;
    let runtime = start_mcp(http(&url), None, PRIVATE).await.unwrap();
    assert!(searched(&runtime).await);

    // Back without the old session, refusing it with 400: the client
    // cannot tell, the ping can.
    stop.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let refusing = Quirk::RefusesUnknownSessions;
    serve_mcp_http_on(port_of(&url), None, refusing).await;
    assert!(!searched(&runtime).await);
    assert!(closes(&runtime).await, "the session never said it was lost");

    // Started again, a session reaches it.
    let again = start_mcp(http(&url), None, PRIVATE).await.unwrap();
    assert!(searched(&again).await);
    again.closer.unwrap().close().await;
}

#[tokio::test]
async fn a_slow_tool_keeps_the_session_of_a_server_that_answers_its_ping() {
    // Answered, or refused as a method it does not know: an answer still.
    for quirk in [Quirk::SlowCall, Quirk::SlowCallPingUnknown] {
        let url = serve_mcp_http(None, quirk).await;
        let runtime = start_mcp_calling_within(http(&url), PRIVATE, CALL)
            .await
            .unwrap();
        let result = call_as_reader(&runtime, "mcp_docs_get_item", json!({})).await;
        let said = result.content.to_string();
        assert!(result.is_error && said.contains(TOO_SLOW), "{said}");
        // Long enough for the ping, not for the call: its session is kept.
        assert!(CALL * 2 < SLOW_CALL);
        tokio::time::sleep(CALL * 2).await;
        assert!(!runtime.is_closed(), "{quirk:?}");
        runtime.closer.unwrap().close().await;
    }
}

/// A local demo server whose process the test knows: its configuration,
/// the file its process id is written to, and the policy that allows it.
fn local_server() -> (Value, PathBuf, NetworkPolicy) {
    let pid_file = std::env::temp_dir().join(format!("mimorya-mcp-{}.pid", uuid::Uuid::new_v4()));
    let config = json!({
        "transport": "stdio",
        "prefix": "docs",
        "command": "sh",
        "args": ["-c", "echo $$ > \"$0\" && exec \"$1\"", pid_file, env!("CARGO_BIN_EXE_mcp-demo-docs")],
    });
    let allowed = NetworkPolicy {
        allow_private_network: false,
        allow_stdio: true,
        proxy: None,
    };
    (config, pid_file, allowed)
}

/// Sends `signal` to the process whose id is in `pid_file`.
fn signal(pid_file: &PathBuf, signal: &str) {
    let pid = std::fs::read_to_string(pid_file).unwrap();
    let sent = std::process::Command::new("kill")
        .args([signal, pid.trim()])
        .status()
        .unwrap();
    assert!(sent.success());
}

#[tokio::test]
async fn a_local_server_that_exits_leaves_a_session_that_says_it_is_closed() {
    let (config, pid_file, allowed) = local_server();
    let runtime = start_mcp(config, None, allowed).await.unwrap();
    assert!(searched(&runtime).await);
    assert!(!runtime.is_closed());

    signal(&pid_file, "-TERM");
    let _ = std::fs::remove_file(&pid_file);
    assert!(closes(&runtime).await, "the session never said it closed");
    assert!(!searched(&runtime).await);
}

#[tokio::test]
async fn a_mute_local_server_loses_its_session() {
    let (config, pid_file, allowed) = local_server();
    let runtime = start_mcp_calling_within(config, allowed, CALL)
        .await
        .unwrap();
    assert!(searched(&runtime).await);

    // Alive, but reading nothing more: the call, then the ping, go
    // unanswered.
    signal(&pid_file, "-STOP");
    let search = json!({ "query": "circulateur" });
    let result = call_as_reader(&runtime, "mcp_docs_search_procedures", search).await;
    let said = result.content.to_string();
    assert!(said.contains(TOO_SLOW), "{said}");
    let lost = closes(&runtime).await;
    signal(&pid_file, "-KILL");
    let _ = std::fs::remove_file(&pid_file);
    assert!(lost, "the session never said it was lost");
}
