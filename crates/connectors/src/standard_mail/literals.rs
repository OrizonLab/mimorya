//! What an IMAP server announces before it sends it (ADR-205). The IMAP
//! library reads the server's bytes with imap-proto: where the parser says
//! it needs N bytes more (a literal `{N}`, wherever its grammar takes one),
//! the library makes room for them at once, before any of them arrives, up
//! to 512 MiB. The guard reads the same bytes with the same parser, as the
//! library reads them, and refuses them where the parser asks for more than
//! the session may still read: its budget spent, the refusal says « réponse
//! trop longue » and nothing is set aside.
//!
//! The parser alone says what is a literal, never a guess at its grammar:
//! no response it reads a literal in escapes the guard (a status's code, an
//! attribute it does not know), and the text of an e-mail is never taken
//! for an announcement. Bytes the parser cannot read are left to the
//! library, which refuses them itself before reading any further.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use imap_proto::Response;
use nom::{Err as Parsed, Needed};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::outbound::{Connection, ReadBudget, TOO_LONG};

/// `connection`, its literals refused past what `budget` still allows.
pub fn guarded(connection: Connection, budget: ReadBudget) -> Connection {
    Box::new(LiteralGuard {
        inner: connection,
        budget,
        mirror: Mirror::default(),
    })
}

#[derive(Debug)]
struct LiteralGuard {
    inner: Connection,
    budget: ReadBudget,
    mirror: Mirror,
}

/// The bytes the library holds and has not read as a response yet, as it
/// holds them.
#[derive(Debug, Default)]
struct Mirror {
    pending: Vec<u8>,
    /// How many bytes the parser needs before it is worth trying again, as
    /// the library counts them.
    needs: usize,
    /// The parser refused these bytes: so does the library, which reads no
    /// further.
    refused: bool,
}

impl Mirror {
    /// Takes `bytes` in; the room the parser then asks for beyond them, if
    /// it asks for a known size.
    fn take(&mut self, bytes: &[u8]) -> Option<usize> {
        if self.refused {
            return None;
        }
        self.pending.extend_from_slice(bytes);
        let mut read = 0;
        let mut asked = None;
        while read < self.pending.len() && self.pending.len() >= self.needs {
            let rest = &self.pending[read..];
            match Response::parse(rest) {
                Ok((left, _)) if left.len() < rest.len() => {
                    read += rest.len() - left.len();
                    self.needs = 0;
                }
                Err(Parsed::Incomplete(Needed::Size(more))) => {
                    let more = usize::from(more);
                    self.needs = self.pending.len() - read + more;
                    asked = Some(more);
                    break;
                }
                Err(Parsed::Incomplete(Needed::Unknown)) => {
                    self.needs = 0;
                    break;
                }
                // A response read from nothing, or bytes the parser refuses.
                Ok(_) | Err(_) => {
                    self.refused = true;
                    self.pending = Vec::new();
                    return None;
                }
            }
        }
        // What the library reads as responses, it lets go of.
        self.pending.drain(..read);
        asked
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
        let asked = this.mirror.take(&buf.filled()[start..]);
        if asked.is_some_and(|more| more > this.budget.left()) {
            this.budget.exhaust();
            // A refused read hands nothing on.
            buf.set_filled(start);
            return Poll::Ready(Err(io::Error::other(TOO_LONG)));
        }
        Poll::Ready(Ok(()))
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
        for sent in [
            &b"* 1 FETCH (UID 1 BODY[] {400000000}\r\naaaa"[..],
            // Leading zeros, as the parser reads them, change nothing.
            b"* 1 FETCH (UID 1 BODY[] {0000000000000000000000400000000}\r\naa",
            // In a status's code or a request to go on: the parser takes a
            // literal there too.
            b"* OK [BADCHARSET ({400000000}\r\n",
            b"A1 NO [BADCHARSET ({400000000}\r\n",
            b"* BYE [BADCHARSET ({400000000}\r\n",
            b"+ [BADCHARSET ({400000000}\r\n",
            // After an attribute the parser does not know, read up to the
            // first parenthesis, line breaks and announcements with it.
            b"* 1 FETCH (UID 1 X-FOO ({35}\r\n))\r\n* 2 FETCH (BODY[] {400000000}\r\n",
        ] {
            let said = String::from_utf8_lossy(sent);
            assert_eq!(through(sent, 1024).await, Err(TOO_LONG.into()), "{said}");
        }
    }

    #[tokio::test]
    async fn a_number_the_parser_cannot_read_is_left_to_the_library() {
        let sent = format!("* 1 FETCH (BODY[] {{{}}}\r\n", "9".repeat(60));
        // The library makes no room for it: it fails on it, as the parser.
        let parsed = Response::parse(sent.as_bytes());
        assert!(matches!(parsed, Err(Parsed::Error(_) | Parsed::Failure(_))));
        assert_eq!(through(sent.as_bytes(), 1024).await, Ok(sent.into_bytes()));
    }

    /// Whether the parser reads `sent` whole, response after response.
    fn read_whole(sent: &[u8]) -> bool {
        let mut mirror = Mirror::default();
        mirror.take(sent).is_none() && !mirror.refused && mirror.pending.is_empty()
    }

    #[tokio::test]
    async fn what_an_e_mail_holds_is_never_an_announcement() {
        let header = "Subject: Relance {999999999999}\r\n\r\n";
        let sent = format!(
            "* 1 FETCH (UID 4 BODY[HEADER.FIELDS (SUBJECT)] {{{}}}\r\n{header})\r\n\
             * 2 FETCH (BODY[] {{22}}\r\n{{999999999999999999}}\r\n BODY[HEADER] {{4}}\r\nab\r\n)\r\n\
             A1 OK fini\r\n",
            header.len()
        );
        assert!(read_whole(sent.as_bytes()));
        assert_eq!(through(sent.as_bytes(), 4096).await, Ok(sent.into_bytes()));
        // A status's text is text.
        let fine = b"* OK [CAPABILITY IMAP4rev1] {12}\r\n* OK {1000}\r\nA1 OK fini\r\n";
        assert!(read_whole(fine));
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
