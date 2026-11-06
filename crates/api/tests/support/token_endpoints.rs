//! A token endpoint that takes every connection and never answers (ADR-171):
//! what a renewal meets at an authorization server gone mute.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::net::TcpListener;

/// The endpoint's address, and how many connections it took.
pub async fn mute() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("port");
    let url = format!("http://{}/token", listener.local_addr().expect("address"));
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            counted.fetch_add(1, Ordering::SeqCst);
            held.push(socket);
        }
    });
    (url, calls)
}
