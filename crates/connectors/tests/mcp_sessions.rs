//! A session with an MCP server kept between runs: an HTTP server that
//! restarts is reached again by the same session (the client opens a new
//! one when the old is unknown), and a local server (stdio) that exits
//! leaves a session that says it is closed, for the platform to start it
//! again.

mod support;

use std::time::Duration;

use mimorya_connectors::ConnectorRuntime;
use mimorya_connectors::mcp::NetworkPolicy;
use mimorya_connectors::testing::serve_stoppable_demo_docs_http;
use serde_json::json;
use support::mcp::{PRIVATE, call_as_reader, http, port_of, start_mcp};

/// Whether a search through the session's tool succeeds.
async fn searched(runtime: &ConnectorRuntime) -> bool {
    let search = json!({ "query": "circulateur" });
    let result = call_as_reader(runtime, "mcp_docs_search_procedures", search).await;
    !result.is_error
}

#[tokio::test]
async fn an_http_session_reaches_its_server_again_after_a_restart() {
    let (url, stop) = serve_stoppable_demo_docs_http(0).await;
    let runtime = start_mcp(http(&url), None, PRIVATE).await.unwrap();
    assert!(searched(&runtime).await);

    // Stopped: calls fail, but the session is not closed for good.
    stop.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!searched(&runtime).await);
    assert!(!runtime.is_closed());

    // Back, without the old session: the same runtime reaches it again.
    let (_, _running) = serve_stoppable_demo_docs_http(port_of(&url)).await;
    assert!(searched(&runtime).await);
    assert!(!runtime.is_closed());
    runtime.closer.unwrap().close().await;
}

#[tokio::test]
async fn a_local_server_that_exits_leaves_a_session_that_says_it_is_closed() {
    // The demo server, its process known to the test, which ends it.
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
    };
    let runtime = start_mcp(config, None, allowed).await.unwrap();
    assert!(searched(&runtime).await);
    assert!(!runtime.is_closed());

    let pid = std::fs::read_to_string(&pid_file).unwrap();
    let _ = std::fs::remove_file(&pid_file);
    let killed = std::process::Command::new("kill")
        .arg(pid.trim())
        .status()
        .unwrap();
    assert!(killed.success());
    let closed = tokio::time::timeout(Duration::from_secs(10), async {
        while !runtime.is_closed() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    assert!(closed.await.is_ok(), "the session never said it closed");
    assert!(!searched(&runtime).await);
}
