//! What companies connect, reached through the operator's proxy (ADR-162):
//! an MCP server and a REST API alike, over plain HTTP (the proxy forwards
//! the request) or HTTPS (the proxy opens a tunnel), the proxy given its
//! credentials; a host of the bypass reached without it. A proxy that
//! refuses is told without its credentials: by its status (407) for plain
//! HTTP, as an unreachable server for a tunnel. The environment's proxies
//! are never followed, and without private addresses allowed, no client is
//! built with a proxy.

mod support;

use std::time::Duration;

use mimorya_connectors::ConnectorRuntime;
use mimorya_connectors::outbound::{NetworkPolicy, OutboundProxy, UNREACHABLE, http_client};
use mimorya_connectors::testing::{serve_demo_docs_http, serve_proxy, serve_rest_http};
use serde_json::json;
use support::mcp::{PRIVATE, call_as_reader, http, start_mcp_within};
use support::rest::{api, echo, start_rest};

const CREDENTIALS: &str = "compte:secret-du-mandataire";
const START: Duration = Duration::from_secs(10);

/// Private addresses allowed, through the proxy at `proxy` (an address
/// without credentials) given `credentials`, `bypass` reached without it.
fn through(proxy: &str, credentials: &str, bypass: Option<&str>) -> NetworkPolicy {
    let address = proxy.replace("http://", &format!("http://{credentials}@"));
    let proxy = OutboundProxy::new(&address, bypass).expect("a proxy");
    NetworkPolicy {
        proxy: Some(proxy.leak()),
        ..PRIVATE
    }
}

/// `host:port` of an address.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split('/').next().unwrap_or_default().to_string()
}

async fn mcp(url: &str, network: NetworkPolicy) -> anyhow::Result<ConnectorRuntime> {
    start_mcp_within(http(url), None, network, START).await
}

#[tokio::test]
async fn an_mcp_server_is_reached_through_the_proxy() {
    let (proxy, proxied) = serve_proxy(Some(CREDENTIALS)).await;
    let url = serve_demo_docs_http(None).await;
    let runtime = mcp(&url, through(&proxy, CREDENTIALS, None)).await.unwrap();
    let search = json!({ "query": "circulateur" });
    let result = call_as_reader(&runtime, "mcp_docs_search_procedures", search).await;
    assert!(!result.is_error, "{}", result.content);
    let targets = proxied.targets();
    assert!(!targets.is_empty(), "every request through the proxy");
    assert!(targets.iter().all(|t| *t == host_of(&url)), "{targets:?}");
    runtime.closer.unwrap().close().await;
}

#[tokio::test]
async fn a_rest_api_is_reached_through_the_proxy() {
    let (proxy, proxied) = serve_proxy(Some(CREDENTIALS)).await;
    let url = serve_rest_http().await;
    let operations = json!([echo("get_item", "GET", "/echo/items", json!([]))]);
    let network = through(&proxy, CREDENTIALS, None);
    let runtime = start_rest(api(&url, operations), None, network, START)
        .await
        .unwrap();
    let result = call_as_reader(&runtime, "rest_crm_get_item", json!({})).await;
    assert!(!result.is_error, "{}", result.content);
    assert_eq!(proxied.targets(), [host_of(&url)]);
}

#[tokio::test]
async fn a_host_of_the_bypass_is_reached_without_the_proxy() {
    let (proxy, proxied) = serve_proxy(Some(CREDENTIALS)).await;
    let url = serve_demo_docs_http(None).await;
    let network = through(&proxy, CREDENTIALS, Some("intranet.example, 127.0.0.1"));
    let runtime = mcp(&url, network).await.unwrap();
    assert!(proxied.targets().is_empty(), "{:?}", proxied.targets());
    runtime.closer.unwrap().close().await;
}

#[tokio::test]
async fn a_proxy_that_refuses_or_does_not_answer_is_told_without_its_credentials() {
    let url = serve_demo_docs_http(None).await;
    // Refused: wrong credentials.
    let (proxy, proxied) = serve_proxy(Some(CREDENTIALS)).await;
    let wrong = "compte:mauvais-secret";
    let error = mcp(&url, through(&proxy, wrong, None))
        .await
        .err()
        .expect("refused");
    let said = format!("{error:#}");
    assert!(said.contains("407"), "{said}");
    assert!(!said.contains("mauvais-secret"), "{said}");
    assert!(proxied.targets().is_empty());
    // Not answering: nothing listens there any more.
    let gone = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let error = mcp(&url, through(&gone, CREDENTIALS, None))
        .await
        .err()
        .expect("unreachable");
    let said = format!("{error:#}");
    assert!(!said.contains("secret-du-mandataire"), "{said}");
}

#[tokio::test]
async fn an_https_server_is_reached_through_a_tunnel() {
    // What every service the client reaches is: a tunnel (`CONNECT`), its
    // credentials given there, the proxy noting where after checking them.
    let (proxy, proxied) = serve_proxy(Some(CREDENTIALS)).await;
    let network = through(&proxy, CREDENTIALS, None);
    // Nothing answers there: where it went is what counts.
    let _ = mcp("https://audit.invalid/mcp", network).await;
    assert_eq!(proxied.targets(), ["audit.invalid:443"]);
}

#[tokio::test]
async fn a_tunnel_the_proxy_refuses_is_told_without_its_credentials() {
    let (proxy, proxied) = serve_proxy(Some(CREDENTIALS)).await;
    let network = through(&proxy, "compte:mauvais-secret", None);
    let error = mcp("https://audit.invalid/mcp", network)
        .await
        .err()
        .expect("refused");
    let said = format!("{error:#}");
    // Refused on the tunnel: no server answered, as said.
    assert!(said.contains(UNREACHABLE), "{said}");
    assert!(said.contains("proxy authorization required"), "{said}");
    assert!(!said.contains("mauvais-secret"), "{said}");
    assert!(proxied.targets().is_empty());
}

/// Where the child process of `the_environment_s_proxies_are_never_followed`
/// sends its request.
const CHILD_TARGET: &str = "MIMORYA_TEST_CHILD_TARGET";

#[tokio::test]
async fn the_environment_s_proxies_are_never_followed() {
    let (proxy, proxied) = serve_proxy(None).await;
    // In a child process: the environment is the process's own.
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", "requested_by_the_child", "--include-ignored"])
        .env(CHILD_TARGET, "http://audit.invalid/")
        .env_remove("NO_PROXY")
        .env_remove("no_proxy");
    for name in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
    ] {
        child.env(name, &proxy);
    }
    let ran = child.output().await.unwrap();
    let said = String::from_utf8_lossy(&ran.stdout);
    // The child ran its request: a filter naming no test would prove nothing.
    assert!(ran.status.success() && said.contains("1 passed"), "{said}");
    assert!(proxied.targets().is_empty(), "{:?}", proxied.targets());
}

/// Run by `the_environment_s_proxies_are_never_followed`, in a child
/// process whose environment names a proxy; nothing to do otherwise.
#[tokio::test]
#[ignore = "run in a child process by the_environment_s_proxies_are_never_followed"]
async fn requested_by_the_child() {
    let Ok(target) = std::env::var(CHILD_TARGET) else {
        return;
    };
    let client = http_client(PRIVATE).unwrap();
    // Nothing answers there: where it went is what counts.
    let _ = client.get(target).send().await;
}

#[test]
fn without_private_addresses_no_client_takes_a_proxy() {
    let proxy = OutboundProxy::new("http://127.0.0.1:3128", None).unwrap();
    let public_only = NetworkPolicy {
        allow_private_network: false,
        allow_stdio: false,
        proxy: Some(proxy.leak()),
    };
    assert!(http_client(public_only).is_err());
}
