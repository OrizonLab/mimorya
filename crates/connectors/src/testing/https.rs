//! An HTTPS server for the tests (ADR-205): an axum router served over TLS
//! with the certificate of a test authority, which a client trusts through
//! its network policy alone. A connection whose handshake fails (a client
//! refusing the certificate) is dropped, and the next one waited for.

use std::io;
use std::net::SocketAddr;

use axum::Router;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;

use super::TestAuthority;

/// Connections accepted over TLS.
struct TlsListener {
    tcp: TcpListener,
    acceptor: TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let Ok((socket, address)) = self.tcp.accept().await else {
                continue;
            };
            if let Ok(secured) = self.acceptor.accept(socket).await {
                return (secured, address);
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}

/// `router` served over HTTPS on a local port, with `authority`'s
/// certificate; its address.
pub async fn serve_https(authority: &TestAuthority, router: Router) -> SocketAddr {
    let tcp = TcpListener::bind("127.0.0.1:0").await.expect("local port");
    let address = tcp.local_addr().expect("local address");
    let listener = TlsListener {
        tcp,
        acceptor: authority.acceptor(),
    };
    tokio::spawn(async move { axum::serve(listener, router).await });
    address
}
