//! A token endpoint that holds its answer until a test lets it go (ADR-171,
//! ADR-174): what changes while a code is being exchanged.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Notify;

/// What a held token endpoint grants once let go: tokens the platform
/// could keep.
pub const GRANTED: &str = r#"{"access_token":"jeton-retenu","token_type":"Bearer",
"expires_in":3600,"refresh_token":"renouvellement-retenu"}"#;

/// A token endpoint that reads the request, then answers only once let go:
/// its address, how many requests it read, and what lets it go.
pub async fn held() -> (String, Arc<AtomicUsize>, Arc<Notify>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("port");
    let url = format!("http://{}/token", listener.local_addr().expect("address"));
    let (read, release) = (Arc::new(AtomicUsize::new(0)), Arc::new(Notify::new()));
    let (counted, gate) = (read.clone(), release.clone());
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = Vec::new();
            let mut chunk = [0u8; 4096];
            // Up to its form, which names the grant.
            while !request.windows(10).any(|w| w == b"grant_type") {
                match socket.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                }
            }
            counted.fetch_add(1, Ordering::SeqCst);
            gate.notified().await;
            let answer = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{GRANTED}",
                GRANTED.len()
            );
            let _ = socket.write_all(answer.as_bytes()).await;
        }
    });
    (url, read, release)
}
