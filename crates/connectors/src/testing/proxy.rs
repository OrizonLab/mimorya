//! An HTTP proxy for the tests (ADR-162): it forwards each connection to
//! the server its first request names (`CONNECT host:port`, or an absolute
//! address) and lists where it went; with credentials, it refuses a
//! request without them (407).

use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};

/// Where the proxy went, one entry per connection: `host:port`.
#[derive(Default)]
pub struct Proxied(Mutex<Vec<String>>);

impl Proxied {
    pub fn targets(&self) -> Vec<String> {
        self.0.lock().expect("proxied").clone()
    }
}

/// Serves a proxy on a local port, `credentials` (`user:password`) required
/// if given; returns its address (`http://127.0.0.1:<port>`) and where it went.
pub async fn serve_proxy(credentials: Option<&str>) -> (String, Arc<Proxied>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("local port");
    let address = format!("http://{}", listener.local_addr().expect("local address"));
    let proxied = Arc::new(Proxied::default());
    let expected = credentials.map(|c| format!("basic {}", STANDARD.encode(c)));
    let log = proxied.clone();
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let (log, expected) = (log.clone(), expected.clone());
            tokio::spawn(async move {
                let _ = forward(client, &log, expected.as_deref()).await;
            });
        }
    });
    (address, proxied)
}

async fn forward(
    mut client: TcpStream,
    log: &Proxied,
    expected: Option<&str>,
) -> std::io::Result<()> {
    let head = read_head(&mut client).await?;
    let text = String::from_utf8_lossy(&head).to_string();
    let mut words = text.lines().next().unwrap_or_default().split_whitespace();
    let (method, target) = (
        words.next().unwrap_or_default(),
        words.next().unwrap_or_default(),
    );
    if let Some(expected) = expected {
        let given = text.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("proxy-authorization")
                .then(|| value.trim().to_lowercase())
        });
        if given.as_deref() != Some(&expected.to_lowercase()) {
            let refused = "HTTP/1.1 407 Proxy Authentication Required\r\ncontent-length: 0\r\n\r\n";
            return client.write_all(refused.as_bytes()).await;
        }
    }
    let host = match method {
        "CONNECT" => target.to_string(),
        _ => target
            .split_once("://")
            .map(|(_, rest)| rest.split('/').next().unwrap_or_default().to_string())
            .unwrap_or_default(),
    };
    log.0.lock().expect("proxied").push(host.clone());
    let mut server = TcpStream::connect(&host).await?;
    if method == "CONNECT" {
        client
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await?;
    } else {
        // An absolute address, as the origin server reads it too.
        server.write_all(&head).await?;
    }
    copy_bidirectional(&mut client, &mut server)
        .await
        .map(|_| ())
}

/// The request's head, up to its blank line.
async fn read_head(client: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") && head.len() < 64 * 1024 {
        if client.read(&mut byte).await? == 0 {
            break;
        }
        head.push(byte[0]);
    }
    Ok(head)
}
