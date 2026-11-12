//! The connections of the mail servers a company names (ADR-205): TLS to a
//! server whose certificate chains to the trusted roots and names it,
//! refused otherwise (another authority, another name, no TLS at all, no
//! answer in time); a private address or name refused unless allowed, a
//! test server reached by its exact address alone, a name no server has
//! told as such; through the operator's proxy, plain or over TLS, given its
//! credentials, refused without them and told without them, a host of its
//! bypass reached without it. A proxy that never opens the tunnel, refuses
//! it, or answers endlessly or unreadably is told so, never waited for
//! longer than its delay nor read further than its bound.

use std::net::SocketAddr;

use mimorya_connectors::outbound::{
    Cause, Connection, INVALID_HOST, NOT_PUBLIC, NOT_SECURE, NetworkPolicy, OutboundProxy,
    PROXY_REFUSED, TOO_SLOW, UNREACHABLE, connect, secured,
};
use mimorya_connectors::testing::{
    TestAuthority, serve_mute, serve_proxy, serve_tls_echo, serve_tls_proxy,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

const LOCAL: NetworkPolicy = NetworkPolicy {
    allow_private_network: true,
    allow_stdio: false,
    proxy: None,
    test_servers: &[],
    test_roots: &[],
};
const SAAS: NetworkPolicy = NetworkPolicy {
    allow_private_network: false,
    ..LOCAL
};
const GREETING: &str = "* OK serveur de test prêt";
const CREDENTIALS: &str = "compte:secret-du-mandataire";

/// Private addresses allowed, the authority's root trusted.
fn trusting(authority: &TestAuthority) -> NetworkPolicy {
    NetworkPolicy {
        test_roots: authority.roots(),
        ..LOCAL
    }
}

/// The same, through the proxy at `proxy` given `credentials`, `bypass`
/// reached without it.
fn through(
    policy: NetworkPolicy,
    proxy: &str,
    credentials: &str,
    bypass: Option<&str>,
) -> NetworkPolicy {
    let address = proxy.replace("://", &format!("://{credentials}@"));
    let proxy = OutboundProxy::new(&address, bypass).expect("a proxy");
    NetworkPolicy {
        proxy: Some(proxy.leak()),
        ..policy
    }
}

/// A secured connection to `host:port`.
async fn reach(host: &str, port: u16, policy: NetworkPolicy) -> Result<Connection, Cause> {
    let plain = connect(host, port, policy).await?;
    secured(plain, host, policy).await
}

/// The server's greeting, then its answer to a line.
async fn exchange(connection: Connection) -> (String, String) {
    let mut lines = BufReader::new(connection);
    let mut greeting = String::new();
    lines.read_line(&mut greeting).await.expect("greeting");
    lines
        .get_mut()
        .write_all(b"NOOP\r\n")
        .await
        .expect("a line");
    let mut answer = String::new();
    lines.read_line(&mut answer).await.expect("answer");
    (greeting.trim_end().into(), answer.trim_end().into())
}

/// A proxy that reads one request's head, says so, then answers `answer`
/// (nothing at all if `None`) and holds the connection; its address.
async fn scripted_proxy(answer: Option<Vec<u8>>) -> (String, oneshot::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let (heard, told) = oneshot::channel();
    tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") && socket.read(&mut byte).await.unwrap_or(0) == 1 {
            head.push(byte[0]);
        }
        let _ = heard.send(());
        if let Some(answer) = answer {
            let _ = socket.write_all(&answer).await;
        }
        let mut held = [0u8; 64];
        while socket.read(&mut held).await.unwrap_or(0) > 0 {}
    });
    (address, told)
}

fn refused<T>(result: Result<T, Cause>) -> Cause {
    match result {
        Ok(_) => panic!("the connection should be refused"),
        Err(cause) => cause,
    }
}

#[tokio::test]
async fn a_server_is_reached_in_tls_when_its_certificate_chains_to_the_roots() {
    let authority = TestAuthority::new();
    let server = serve_tls_echo(&authority, GREETING).await;
    for host in ["127.0.0.1", "localhost"] {
        let connection = reach(host, server.port(), trusting(&authority))
            .await
            .unwrap();
        let (greeting, answer) = exchange(connection).await;
        assert_eq!(greeting, GREETING, "{host}");
        assert_eq!(answer, "echo: NOOP", "{host}");
    }
}

#[tokio::test]
async fn a_certificate_of_another_authority_or_for_another_name_is_refused() {
    let authority = TestAuthority::new();
    let server = serve_tls_echo(&authority, GREETING).await;
    // The webpki roots alone, then another authority's.
    let untrusted = refused(reach("127.0.0.1", server.port(), LOCAL).await);
    assert_eq!(untrusted.summary, NOT_SECURE, "{untrusted:?}");
    let other = TestAuthority::new();
    let foreign = refused(reach("127.0.0.1", server.port(), trusting(&other)).await);
    assert_eq!(foreign.summary, NOT_SECURE, "{foreign:?}");
    // A trusted certificate naming another server.
    let elsewhere = TestAuthority::naming(&["mail.example.com"]);
    let server = serve_tls_echo(&elsewhere, GREETING).await;
    let named = refused(reach("127.0.0.1", server.port(), trusting(&elsewhere)).await);
    assert_eq!(named.summary, NOT_SECURE, "{named:?}");
}

#[tokio::test]
async fn a_server_speaking_in_clear_is_refused() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let _ = socket.write_all(b"* OK en clair\r\n").await;
            tokio::spawn(async move {
                let mut held = [0u8; 1024];
                let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut held).await;
            });
        }
    });
    let authority = TestAuthority::new();
    let cause = refused(reach("127.0.0.1", port, trusting(&authority)).await);
    assert_eq!(cause.summary, NOT_SECURE, "{cause:?}");
}

#[tokio::test]
async fn a_server_that_never_answers_the_handshake_is_too_slow() {
    let mute = serve_mute().await;
    let port: u16 = mute
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches("/mcp")
        .parse()
        .unwrap();
    let plain = connect("127.0.0.1", port, LOCAL).await.unwrap();
    // The connection is open: the handshake's delay alone runs out.
    tokio::time::pause();
    let cause = refused(secured(plain, "127.0.0.1", LOCAL).await);
    assert_eq!(cause.summary, TOO_SLOW, "{cause:?}");
}

#[tokio::test]
async fn a_private_address_or_name_is_refused_unless_allowed() {
    let authority = TestAuthority::new();
    let server = serve_tls_echo(&authority, GREETING).await;
    for host in ["127.0.0.1", "localhost"] {
        let cause = refused(connect(host, server.port(), SAAS).await);
        assert_eq!(cause.summary, NOT_PUBLIC, "{host}: {cause:?}");
    }
    // A test server, by its exact address alone.
    let tested = NetworkPolicy {
        test_servers: Vec::leak(vec![server]),
        test_roots: authority.roots(),
        ..SAAS
    };
    let connection = reach("127.0.0.1", server.port(), tested).await.unwrap();
    assert_eq!(exchange(connection).await.0, GREETING);
    let cause = refused(connect("localhost", server.port(), tested).await);
    assert_eq!(cause.summary, NOT_PUBLIC, "{cause:?}");
    let other = SocketAddr::new(server.ip(), server.port().wrapping_add(1).max(1));
    let cause = refused(connect("127.0.0.1", other.port(), tested).await);
    assert_eq!(cause.summary, NOT_PUBLIC, "{cause:?}");
}

#[tokio::test]
async fn a_name_no_server_has_is_told_as_such() {
    for (host, port) in [("imap..example.com", 993), ("imap.example.com", 0)] {
        let cause = refused(connect(host, port, LOCAL).await);
        assert_eq!(cause.summary, INVALID_HOST, "{host}:{port}: {cause:?}");
    }
}

#[tokio::test]
async fn a_server_is_reached_through_the_proxy_given_its_credentials() {
    let authority = TestAuthority::new();
    let server = serve_tls_echo(&authority, GREETING).await;
    let (proxy, proxied) = serve_proxy(Some(CREDENTIALS)).await;
    let policy = through(trusting(&authority), &proxy, CREDENTIALS, None);
    let connection = reach("127.0.0.1", server.port(), policy).await.unwrap();
    assert_eq!(
        exchange(connection).await,
        (GREETING.into(), "echo: NOOP".into())
    );
    assert_eq!(proxied.targets(), vec![server.to_string()]);
}

#[tokio::test]
async fn a_proxy_refusing_its_credentials_is_told_without_them() {
    let authority = TestAuthority::new();
    let server = serve_tls_echo(&authority, GREETING).await;
    let (proxy, proxied) = serve_proxy(Some(CREDENTIALS)).await;
    let wrong = "compte:autre-secret";
    let policy = through(trusting(&authority), &proxy, wrong, None);
    let cause = refused(connect("127.0.0.1", server.port(), policy).await);
    assert_eq!(cause.summary, PROXY_REFUSED, "{cause:?}");
    assert!(cause.detail.contains("407"), "{cause:?}");
    let told = format!("{cause:?}");
    assert!(
        !told.contains("secret") && !told.contains("compte"),
        "{told}"
    );
    assert!(proxied.targets().is_empty());
}

#[tokio::test]
async fn a_host_of_the_bypass_is_reached_without_the_proxy() {
    let authority = TestAuthority::new();
    let server = serve_tls_echo(&authority, GREETING).await;
    let (proxy, proxied) = serve_proxy(Some(CREDENTIALS)).await;
    let policy = through(trusting(&authority), &proxy, CREDENTIALS, Some("127.0.0.1"));
    let connection = reach("127.0.0.1", server.port(), policy).await.unwrap();
    assert_eq!(exchange(connection).await.0, GREETING);
    assert!(proxied.targets().is_empty(), "{:?}", proxied.targets());
}

#[tokio::test]
async fn an_https_proxy_is_reached_over_tls_with_a_trusted_certificate() {
    let authority = TestAuthority::new();
    let server = serve_tls_echo(&authority, GREETING).await;
    let (proxy, proxied) = serve_tls_proxy(Some(CREDENTIALS), &authority).await;
    assert!(proxy.starts_with("https://"), "{proxy}");
    let policy = through(trusting(&authority), &proxy, CREDENTIALS, None);
    let connection = reach("127.0.0.1", server.port(), policy).await.unwrap();
    assert_eq!(exchange(connection).await.1, "echo: NOOP");
    assert_eq!(proxied.targets(), vec![server.to_string()]);
    // A proxy whose certificate the roots do not vouch for.
    let other = TestAuthority::new();
    let (proxy, proxied) = serve_tls_proxy(Some(CREDENTIALS), &other).await;
    let policy = through(trusting(&authority), &proxy, CREDENTIALS, None);
    let cause = refused(connect("127.0.0.1", server.port(), policy).await);
    assert_eq!(cause.summary, NOT_SECURE, "{cause:?}");
    // Said as the proxy's certificate, not the mail server's.
    assert!(cause.detail.starts_with("mandataire"), "{cause:?}");
    assert!(proxied.targets().is_empty());
}

#[tokio::test]
async fn a_proxy_that_never_opens_the_tunnel_is_too_slow() {
    let (proxy, heard) = scripted_proxy(None).await;
    let policy = through(LOCAL, &proxy, CREDENTIALS, None);
    let attempt = tokio::spawn(async move { connect("imap.example.com", 993, policy).await });
    // Asked: from there, only the tunnel's delay runs out.
    heard.await.unwrap();
    tokio::time::pause();
    let cause = refused(attempt.await.unwrap());
    assert_eq!(cause.summary, TOO_SLOW, "{cause:?}");
    assert!(cause.detail.contains("tunnel"), "{cause:?}");
}

#[tokio::test]
async fn a_tunnel_the_proxy_refuses_is_told_by_its_code_alone() {
    for code in ["403 Forbidden", "502 Bad Gateway"] {
        let answer = format!("HTTP/1.1 {code}\r\nX-Detail: secret-du-mandataire\r\n\r\n");
        let (proxy, _) = scripted_proxy(Some(answer.into_bytes())).await;
        let policy = through(LOCAL, &proxy, CREDENTIALS, None);
        let cause = refused(connect("imap.example.com", 993, policy).await);
        assert_eq!(cause.summary, PROXY_REFUSED, "{cause:?}");
        let number = &code[..3];
        assert_eq!(cause.detail, format!("réponse {number}"), "{cause:?}");
    }
}

#[tokio::test]
async fn an_endless_or_unreadable_proxy_answer_is_cut_short() {
    // More than the bound, without the blank line that ends a head.
    let endless = format!("HTTP/1.1 200 {}", "a".repeat(9 * 1024)).into_bytes();
    let unreadable = b"SSH-2.0-OpenSSH_9.6\r\n\r\n".to_vec();
    for (answer, said) in [(endless, "incomplète"), (unreadable, "illisible")] {
        let (proxy, _) = scripted_proxy(Some(answer)).await;
        let policy = through(LOCAL, &proxy, CREDENTIALS, None);
        let cause = refused(connect("imap.example.com", 993, policy).await);
        assert_eq!(cause.summary, UNREACHABLE, "{cause:?}");
        assert!(cause.detail.contains(said), "{cause:?}");
        let told = format!("{cause:?}");
        assert!(
            !told.contains("secret") && !told.contains("compte"),
            "{told}"
        );
    }
}

#[tokio::test]
async fn without_private_addresses_no_connection_takes_a_proxy() {
    let proxy = OutboundProxy::new("http://127.0.0.1:3128", None).unwrap();
    let public_only = NetworkPolicy {
        proxy: Some(proxy.leak()),
        ..SAAS
    };
    let cause = refused(connect("imap.example.com", 993, public_only).await);
    assert_eq!(cause.summary, UNREACHABLE, "{cause:?}");
    assert!(cause.detail.contains("mandataire"), "{cause:?}");
}
