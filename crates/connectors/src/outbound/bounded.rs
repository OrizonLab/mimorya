//! A connection that reads no more than its budget (ADR-205): what a
//! company's server sends over a connection the platform opened is bounded,
//! as an HTTP answer is (ADR-161), so that a server answering without end
//! fills no memory. Once spent, a read fails; the caller tells it as an
//! answer too long (`TOO_LONG`). A step allowed to read more (an e-mail) is
//! granted it on the budget's handle.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::tcp::Connection;

/// What a server that sent more than it may is told.
pub const TOO_LONG: &str = "Le serveur a envoyé une réponse trop longue";
/// The most a single read takes at once.
const CHUNK_BYTES: usize = 16 * 1024;

/// The bytes a bounded connection may still read.
#[derive(Debug, Clone)]
pub struct ReadBudget(Arc<AtomicUsize>);

impl ReadBudget {
    /// Whether it ran out: a read was refused.
    pub fn spent(&self) -> bool {
        self.0.load(Ordering::Acquire) == 0
    }

    /// The bytes that may still be read.
    pub fn left(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }

    /// Nothing more may be read: what is coming is longer than allowed.
    pub fn exhaust(&self) {
        self.0.store(0, Ordering::Release);
    }

    /// `bytes` more may be read.
    pub fn grant(&self, bytes: usize) {
        let mut left = self.0.load(Ordering::Acquire);
        loop {
            let raised = left.saturating_add(bytes);
            match self
                .0
                .compare_exchange_weak(left, raised, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return,
                Err(now) => left = now,
            }
        }
    }
}

/// `connection`, reading `limit` bytes at most, and its budget.
pub fn bounded(connection: Connection, limit: usize) -> (Connection, ReadBudget) {
    let budget = ReadBudget(Arc::new(AtomicUsize::new(limit)));
    let reader = Bounded {
        inner: connection,
        left: budget.clone(),
        chunk: Vec::new(),
    };
    (Box::new(reader), budget)
}

#[derive(Debug)]
struct Bounded {
    inner: Connection,
    left: ReadBudget,
    /// Where a read lands before it is counted and handed on.
    chunk: Vec<u8>,
}

impl AsyncRead for Bounded {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let left = self.left.0.load(Ordering::Acquire);
        let wanted = left.min(buf.remaining()).min(CHUNK_BYTES);
        if wanted == 0 {
            return match left {
                0 => Poll::Ready(Err(io::Error::other(TOO_LONG))),
                _ => Poll::Ready(Ok(())),
            };
        }
        let this = &mut *self;
        this.chunk.resize(wanted, 0);
        let mut landing = ReadBuf::new(&mut this.chunk);
        ready!(Pin::new(&mut *this.inner).poll_read(cx, &mut landing))?;
        let read = landing.filled().len();
        buf.put_slice(&this.chunk[..read]);
        this.left.0.fetch_sub(read, Ordering::AcqRel);
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for Bounded {
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

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn reads_up_to_its_budget_then_refuses() {
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        let (mut connection, budget) = bounded(Box::new(client), 10);
        server.write_all(&[b'a'; 32]).await.unwrap();
        let mut read = Vec::new();
        let error = connection.read_to_end(&mut read).await.unwrap_err();
        assert_eq!(read.len(), 10);
        assert_eq!(error.to_string(), TOO_LONG);
        assert!(budget.spent());
        // Granted more, it reads on.
        budget.grant(5);
        let mut more = [0u8; 8];
        assert_eq!(connection.read(&mut more).await.unwrap(), 5);
        assert!(budget.spent());
    }

    #[tokio::test]
    async fn an_answer_within_the_budget_reads_whole() {
        let (client, mut server) = tokio::io::duplex(1024);
        let (mut connection, budget) = bounded(Box::new(client), 100);
        server.write_all(b"* OK pret\r\n").await.unwrap();
        drop(server);
        let mut read = String::new();
        connection.read_to_string(&mut read).await.unwrap();
        assert_eq!(read, "* OK pret\r\n");
        assert!(!budget.spent());
    }
}
