//! The platform's connection as lettre takes one (ADR-205): secured already,
//! and, after a STARTTLS the platform said itself, preceded by the greeting
//! lettre reads before its `EHLO`, which the server said in clear and does
//! not repeat.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use lettre::transport::smtp::client::AsyncTokioStream;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::outbound::Connection;

/// What lettre reads first after STARTTLS: the greeting it awaits.
const RESUMED: &[u8] = b"220 suite en TLS\r\n";

#[derive(Debug)]
pub(super) struct Wire {
    inner: Connection,
    /// What is read before the connection itself.
    first: &'static [u8],
}

impl Wire {
    /// A connection over which the server greets.
    pub(super) fn greeted_by_server(inner: Connection) -> Self {
        Self { inner, first: b"" }
    }

    /// A connection the server greeted in clear, before STARTTLS.
    pub(super) fn resumed_after_starttls(inner: Connection) -> Self {
        Self {
            inner,
            first: RESUMED,
        }
    }
}

impl AsyncRead for Wire {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if !self.first.is_empty() {
            let taken = self.first.len().min(buf.remaining());
            buf.put_slice(&self.first[..taken]);
            self.first = &self.first[taken..];
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut *self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Wire {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_shutdown(cx)
    }
}

impl AsyncTokioStream for Wire {
    /// Through a tunnel, the server's address is the proxy's to know; lettre
    /// asks it of nothing it does itself.
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Err(io::Error::other(
            "adresse tenue par la connexion de la plateforme",
        ))
    }
}
