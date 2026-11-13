//! What an IMAP server announces before it sends it (ADR-205): a literal,
//! `{N}` at the end of a line, then N bytes. The IMAP library makes room
//! for the N bytes as soon as it reads the announcement, before any of them
//! arrives, up to 512 MiB: a server announcing more than the session may
//! still read is refused there, its budget spent, so that the refusal says
//! « réponse trop longue » and nothing is set aside for it. The bytes of a
//! literal are passed through unread: what an e-mail holds is never taken
//! for an announcement.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::outbound::{Connection, ReadBudget, TOO_LONG};

/// The longest announcement read: `{` and twenty digits, `}`, CRLF.
const MAX_ANNOUNCEMENT: usize = 24;

/// `connection`, its literals refused past what `budget` still allows.
pub fn guarded(connection: Connection, budget: ReadBudget) -> Connection {
    Box::new(LiteralGuard {
        inner: connection,
        budget,
        literal: 0,
        tail: Vec::with_capacity(MAX_ANNOUNCEMENT),
    })
}

#[derive(Debug)]
struct LiteralGuard {
    inner: Connection,
    budget: ReadBudget,
    /// The bytes of the literal being passed through.
    literal: u64,
    /// The end of the line being read, where an announcement would be.
    tail: Vec<u8>,
}

impl LiteralGuard {
    /// Reads `bytes`; an announcement past the budget refuses them.
    fn watch(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut rest = bytes;
        while !rest.is_empty() {
            if self.literal > 0 {
                let skipped = rest
                    .len()
                    .min(usize::try_from(self.literal).unwrap_or(usize::MAX));
                self.literal -= skipped as u64;
                rest = &rest[skipped..];
                continue;
            }
            let byte = rest[0];
            rest = &rest[1..];
            if self.tail.len() == MAX_ANNOUNCEMENT {
                self.tail.remove(0);
            }
            self.tail.push(byte);
            if byte != b'\n' {
                continue;
            }
            let announced = announced(&self.tail);
            self.tail.clear();
            if let Some(size) = announced {
                if size > self.budget.left() as u64 {
                    self.budget.exhaust();
                    return Err(io::Error::other(TOO_LONG));
                }
                self.literal = size;
            }
        }
        Ok(())
    }
}

/// The size a line's end announces (`{N}` CRLF), if it announces one.
fn announced(tail: &[u8]) -> Option<u64> {
    let line = tail.strip_suffix(b"}\r\n")?;
    let open = line.iter().rposition(|byte| *byte == b'{')?;
    let digits = &line[open + 1..];
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    // Too many digits to parse: more than any budget.
    Some(
        std::str::from_utf8(digits)
            .ok()?
            .parse()
            .unwrap_or(u64::MAX),
    )
}

impl AsyncRead for LiteralGuard {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let start = buf.filled().len();
        ready!(Pin::new(&mut *self.inner).poll_read(cx, buf))?;
        let read = buf.filled()[start..].to_vec();
        Poll::Ready(self.watch(&read))
    }
}

impl AsyncWrite for LiteralGuard {
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
    use crate::outbound::bounded;

    #[test]
    fn reads_an_announcement_at_a_line_s_end_only() {
        assert_eq!(announced(b"* 1 FETCH (BODY[] {42}\r\n"), Some(42));
        assert_eq!(announced(b"{99999999999999999999}\r\n"), Some(u64::MAX));
        assert_eq!(announced(b"* OK \"{12}\"\r\n"), None);
        assert_eq!(announced(b"* OK {}\r\n"), None);
        assert_eq!(announced(b"* OK {1x}\r\n"), None);
        assert_eq!(announced(b"{12}\n"), None);
    }

    #[tokio::test]
    async fn a_literal_past_the_budget_is_refused_before_it_comes() {
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        let (connection, budget) = bounded(Box::new(client), 1024);
        let mut guarded = guarded(connection, budget.clone());
        server
            .write_all(b"* 1 FETCH (UID 1 BODY[] {400000000}\r\naaaa")
            .await
            .unwrap();
        let mut read = vec![0; 256];
        let error = guarded.read(&mut read).await.unwrap_err();
        assert_eq!(error.to_string(), TOO_LONG);
        assert!(budget.spent());
    }

    #[tokio::test]
    async fn a_literal_within_the_budget_passes_its_bytes_unread() {
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        let (connection, budget) = bounded(Box::new(client), 4096);
        let mut guarded = guarded(connection, budget);
        // The literal holds what would be an announcement past the budget.
        let sent = b"* 1 FETCH (BODY[] {22}\r\n{999999999999999999}\r\n)\r\nA1 OK fini\r\n";
        server.write_all(sent).await.unwrap();
        drop(server);
        let mut read = Vec::new();
        guarded.read_to_end(&mut read).await.unwrap();
        assert_eq!(read, sent);
    }
}
