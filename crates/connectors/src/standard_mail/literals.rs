//! What an IMAP server announces before it sends it (ADR-205): a literal,
//! `{N}` at the end of a line of a data response, then N bytes. The IMAP
//! library makes room for the N bytes as soon as it reads the announcement,
//! before any of them arrives, up to 512 MiB: a server announcing more than
//! the session may still read is refused there, its budget spent, so that
//! the refusal says « réponse trop longue » and nothing is set aside for it.
//!
//! The announcement is read as the library reads it: digits of any number
//! (leading zeros too), counted without a bound; never at the end of a
//! status response (`OK`, `NO`, `BAD`, `BYE`, `PREAUTH`) or of a request to
//! go on (`+`, with or without a space), whose text the library does not
//! take for a literal. The
//! bytes of a literal are passed through unread: what an e-mail holds is
//! never taken for an announcement.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::outbound::{Connection, ReadBudget, TOO_LONG};

/// The longest word a status response starts with (`PREAUTH`).
const MAX_STATUS_WORD: usize = 7;

/// `connection`, its literals refused past what `budget` still allows.
pub fn guarded(connection: Connection, budget: ReadBudget) -> Connection {
    Box::new(LiteralGuard {
        inner: connection,
        budget,
        scan: Scan::default(),
    })
}

#[derive(Debug)]
struct LiteralGuard {
    inner: Connection,
    budget: ReadBudget,
    scan: Scan,
}

/// Where a line ending might announce a literal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Announcement {
    #[default]
    None,
    /// Within `{`, the digits read so far.
    Digits { size: u64, any: bool },
    /// `{N}` read.
    Closed(u64),
    /// `{N}` CR read.
    Return(u64),
}

/// What is known of the response being read.
#[derive(Debug, Default)]
struct Scan {
    /// Bytes of the literal being passed through, unread.
    literal: u64,
    /// The words of the response read so far: 0 within the first, 1 within
    /// the second, 2 past it.
    word: u8,
    /// The first byte of the response: `+` asks to go on, with or without
    /// a space after it.
    first: Option<u8>,
    /// The second word, upper-cased, while it may still be a status.
    second: Vec<u8>,
    announcement: Announcement,
}

impl Scan {
    /// Whether the response is a status or a request to go on, whose text
    /// is never a literal.
    fn is_status(&self) -> bool {
        if self.first == Some(b'+') {
            return true;
        }
        self.word >= 2
            && matches!(
                self.second.as_slice(),
                b"OK" | b"NO" | b"BAD" | b"BYE" | b"PREAUTH"
            )
    }

    fn word_byte(&mut self, byte: u8) {
        if self.first.is_none() {
            self.first = Some(byte);
        }
        match (self.word, byte) {
            (0 | 1, b' ') => self.word += 1,
            (1, _) if self.second.len() <= MAX_STATUS_WORD => {
                self.second.push(byte.to_ascii_uppercase());
            }
            _ => {}
        }
    }

    /// A response ended: the next starts.
    fn next_response(&mut self) {
        self.word = 0;
        self.first = None;
        self.second.clear();
        self.announcement = Announcement::None;
    }

    /// The size a line ending announces, after `byte`.
    fn announce(&mut self, byte: u8) -> Option<u64> {
        use Announcement::{Closed, Digits, Return};
        let (next, announced) = match (self.announcement, byte) {
            (_, b'{') => (
                Digits {
                    size: 0,
                    any: false,
                },
                None,
            ),
            (Digits { size, .. }, b'0'..=b'9') => {
                let digit = u64::from(byte - b'0');
                let size = size.saturating_mul(10).saturating_add(digit);
                (Digits { size, any: true }, None)
            }
            (Digits { size, any: true }, b'}') => (Closed(size), None),
            (Closed(size), b'\r') => (Return(size), None),
            (Return(size), b'\n') => (Announcement::None, Some(size)),
            _ => (Announcement::None, None),
        };
        self.announcement = next;
        announced
    }
}

impl LiteralGuard {
    /// Reads `bytes`; an announcement past the budget refuses them.
    fn watch(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut rest = bytes;
        while !rest.is_empty() {
            let scan = &mut self.scan;
            if scan.literal > 0 {
                let left = usize::try_from(scan.literal).unwrap_or(usize::MAX);
                let skipped = rest.len().min(left);
                scan.literal -= skipped as u64;
                rest = &rest[skipped..];
                continue;
            }
            let byte = rest[0];
            rest = &rest[1..];
            let announced = scan.announce(byte);
            scan.word_byte(byte);
            match announced {
                Some(size) if !scan.is_status() => {
                    if size > self.budget.left() as u64 {
                        self.budget.exhaust();
                        return Err(io::Error::other(TOO_LONG));
                    }
                    // The response goes on after the literal.
                    scan.literal = size;
                }
                _ if byte == b'\n' => scan.next_response(),
                _ => {}
            }
        }
        Ok(())
    }
}

impl AsyncRead for LiteralGuard {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let start = buf.filled().len();
        let this = &mut *self;
        ready!(Pin::new(&mut *this.inner).poll_read(cx, buf))?;
        let watched = this.watch(&buf.filled()[start..]);
        if watched.is_err() {
            // A refused read hands nothing on.
            buf.set_filled(start);
        }
        Poll::Ready(watched)
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

    /// What `sent` gives through a guard allowing `budget` bytes: the bytes
    /// read, or the refusal.
    async fn through(sent: &[u8], budget: usize) -> Result<Vec<u8>, String> {
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        let (connection, left) = bounded(Box::new(client), budget);
        let mut guarded = guarded(connection, left);
        server.write_all(sent).await.unwrap();
        drop(server);
        let mut read = Vec::new();
        match guarded.read_to_end(&mut read).await {
            Ok(_) => Ok(read),
            Err(error) => Err(error.to_string()),
        }
    }

    #[tokio::test]
    async fn a_literal_past_the_budget_is_refused_before_it_comes() {
        let sent = b"* 1 FETCH (UID 1 BODY[] {400000000}\r\naaaa";
        assert_eq!(through(sent, 1024).await, Err(TOO_LONG.into()));
        // Leading zeros, as the IMAP library reads them, change nothing.
        let zeros = b"* 1 FETCH (UID 1 BODY[] {0000000000000000000000400000000}\r\naa";
        assert_eq!(through(zeros, 1024).await, Err(TOO_LONG.into()));
        let endless = format!("* 1 FETCH (BODY[] {{{}}}\r\n", "9".repeat(60));
        assert_eq!(
            through(endless.as_bytes(), 1024).await,
            Err(TOO_LONG.into())
        );
    }

    #[tokio::test]
    async fn a_literal_within_the_budget_passes_its_bytes_unread() {
        // The literal holds what would be an announcement past the budget.
        let sent: &[u8] = b"* 1 FETCH (BODY[] {22}\r\n{999999999999999999}\r\n \
BODY[HEADER] {4}\r\nab\r\n)\r\nA1 OK fini\r\n";
        assert_eq!(through(sent, 4096).await, Ok(sent.to_vec()));
    }

    #[tokio::test]
    async fn a_status_never_announces_a_literal() {
        // Text the library reads as text: it hides nothing that follows.
        for status in [
            &b"* OK {1000}\r\n"[..],
            b"A3 NO [ALERT] {1000}\r\n",
            b"a-very-long-tag-the-server-chose-itself OK {1000}\r\n",
            b"+ {1000}\r\n",
            b"+{1000}\r\n",
        ] {
            let mut sent = status.to_vec();
            sent.extend_from_slice(b"* 1 FETCH (UID 8 BODY[] {400000000}\r\n");
            let refused = through(&sent, 4096).await;
            assert_eq!(refused, Err(TOO_LONG.into()), "{status:?}");
        }
        let fine = b"* OK [CAPABILITY IMAP4rev1] {12}\r\nA1 OK fini\r\n";
        assert_eq!(through(fine, 4096).await, Ok(fine.to_vec()));
    }

    #[tokio::test]
    async fn an_announcement_split_across_reads_is_still_seen() {
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        let (connection, left) = bounded(Box::new(client), 1024);
        let mut guarded = guarded(connection, left.clone());
        let mut read = vec![0; 64];
        for part in [&b"* 1 FETCH (BODY[] {40"[..], b"00000", b"00}\r", b"\n"] {
            server.write_all(part).await.unwrap();
            if let Err(error) = guarded.read(&mut read).await {
                assert_eq!(error.to_string(), TOO_LONG);
                assert!(left.spent());
                return;
            }
        }
        panic!("the announcement was not refused");
    }
}
