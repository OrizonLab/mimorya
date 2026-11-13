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
//! for an announcement. Bytes the parser cannot read, a structure nested
//! past its bound among them (`vendor/imap-proto`), are refused as
//! unreadable: the library would refuse them, and never reads them.

use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use async_imap::imap_proto::Response;
use nom::{Err as Parsed, Needed};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::NOT_A_MAIL_SERVER;
use crate::outbound::{Connection, ReadBudget, TOO_LONG, UNREACHABLE};

/// What the guard says of bytes the parser cannot read.
pub const UNREADABLE: &str = "réponse illisible";

/// `connection`, its literals refused past what `budget` still allows.
pub fn guarded(connection: Connection, budget: ReadBudget) -> Connection {
    Box::new(LiteralGuard {
        inner: connection,
        budget,
        mirror: Mirror::default(),
    })
}

/// What a failure to read a guarded connection says: the server's bytes
/// refused as unreadable, or the connection lost.
pub fn io_failure(error: &io::Error) -> (&'static str, String) {
    match error
        .get_ref()
        .is_some_and(|inner| inner.is::<Unreadable>())
    {
        true => (NOT_A_MAIL_SERVER, UNREADABLE.into()),
        false => (UNREACHABLE, error.to_string()),
    }
}

/// The guard's refusal of bytes the parser cannot read.
#[derive(Debug)]
struct Unreadable;

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(UNREADABLE)
    }
}

impl std::error::Error for Unreadable {}

#[derive(Debug)]
struct LiteralGuard {
    inner: Connection,
    budget: ReadBudget,
    mirror: Mirror,
}

/// What the parser makes of the bytes the library holds.
#[derive(Debug, PartialEq, Eq)]
enum Reading {
    /// Responses read, or more bytes awaited, of a size not yet known.
    Read,
    /// A known number of bytes more asked for: the library makes room.
    Asks(usize),
    /// Bytes the parser refuses: so would the library.
    Unreadable,
}

/// The bytes the library holds and has not read as a response yet, as it
/// holds them.
#[derive(Debug, Default)]
struct Mirror {
    pending: Vec<u8>,
    /// How many bytes the parser needs before it is worth trying again, as
    /// the library counts them.
    needs: usize,
    /// The parser refused bytes: every read after is refused too.
    refused: bool,
}

impl Mirror {
    /// Takes `bytes` in; what the parser then makes of what is held.
    fn take(&mut self, bytes: &[u8]) -> Reading {
        if self.refused {
            return Reading::Unreadable;
        }
        self.pending.extend_from_slice(bytes);
        let mut read = 0;
        let mut reading = Reading::Read;
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
                    reading = Reading::Asks(more);
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
                    return Reading::Unreadable;
                }
            }
        }
        // What the library reads as responses, it lets go of.
        self.pending.drain(..read);
        reading
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
        let refused = match this.mirror.take(&buf.filled()[start..]) {
            Reading::Asks(more) if more > this.budget.left() => {
                this.budget.exhaust();
                io::Error::other(TOO_LONG)
            }
            Reading::Unreadable => io::Error::new(io::ErrorKind::InvalidData, Unreadable),
            Reading::Asks(_) | Reading::Read => return Poll::Ready(Ok(())),
        };
        // A refused read hands nothing on.
        buf.set_filled(start);
        Poll::Ready(Err(refused))
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

    /// A body structure `depth` bodies deep, as servers write one.
    fn nested(depth: usize) -> String {
        let mut body = r#"("TEXT" "PLAIN" ("CHARSET" "UTF-8") NIL NIL "7BIT" 12 1)"#.to_string();
        for _ in 1..depth {
            body = format!("({body} \"MIXED\")");
        }
        format!("* 1 FETCH (UID 4 BODYSTRUCTURE {body})\r\n")
    }

    /// A body structure `depth` e-mails deep, each joined to the one before
    /// it (`MESSAGE/RFC822`), its envelope empty.
    fn joined(depth: usize) -> String {
        let mut body = r#"("TEXT" "PLAIN" ("CHARSET" "UTF-8") NIL NIL "7BIT" 12 1)"#.to_string();
        for _ in 1..depth {
            body = format!(
                r#"("MESSAGE" "RFC822" NIL NIL NIL "7BIT" 0 (NIL NIL NIL NIL NIL NIL NIL NIL NIL NIL) {body} 0)"#
            );
        }
        format!("* 1 FETCH (UID 4 BODYSTRUCTURE {body})\r\n")
    }

    /// Whether the parser refuses `sent` for its nesting, past its bound.
    fn too_deep(sent: &[u8]) -> bool {
        let parsed = Response::parse(sent);
        matches!(parsed, Err(Parsed::Failure(e)) if e.code == nom::error::ErrorKind::TooLarge)
    }

    #[tokio::test]
    async fn a_structure_nested_within_the_bound_is_read_past_it_refused() {
        let max = async_imap::imap_proto::nesting::MAX_NESTING;
        // Bodies within a body, e-mails joined to an e-mail.
        for structure in [nested as fn(usize) -> String, joined] {
            let within = structure(max);
            assert!(read_whole(within.as_bytes()), "{within:.80}");
            let read = through(within.as_bytes(), 8192).await;
            assert_eq!(read, Ok(within.into_bytes()));
            let past = structure(max + 1);
            assert!(too_deep(past.as_bytes()), "{past:.80}");
            let refused = through(past.as_bytes(), 8192).await;
            assert_eq!(refused, Err(UNREADABLE.into()));
        }
    }

    #[tokio::test]
    async fn parentheses_without_end_are_refused_not_the_process() {
        // Bodies within bodies, extensions within an extension: the parser
        // would call itself for each, past what the stack holds.
        let bodies = format!("* 1 FETCH (BODYSTRUCTURE {}", "(".repeat(1000));
        let extensions = format!(
            "* 1 FETCH (BODYSTRUCTURE (\"TEXT\" \"PLAIN\" NIL NIL NIL \"7BIT\" 0 0 NIL NIL NIL NIL {}",
            "(".repeat(1000)
        );
        for sent in [bodies, extensions] {
            assert!(too_deep(sent.as_bytes()), "{sent:.60}");
            assert_eq!(through(sent.as_bytes(), 8192).await, Err(UNREADABLE.into()));
        }
    }

    #[tokio::test]
    async fn what_the_parser_cannot_read_is_refused_and_all_after_it() {
        let sent = format!("* 1 FETCH (BODY[] {{{}}}\r\n", "9".repeat(60));
        assert!(Response::parse(sent.as_bytes()).is_err());
        let (client, mut server) = tokio::io::duplex(64 * 1024);
        let (connection, left) = bounded(Box::new(client), 1024);
        let mut guarded = guarded(connection, left);
        let mut read = vec![0; 256];
        server.write_all(sent.as_bytes()).await.unwrap();
        let refused = guarded.read(&mut read).await.unwrap_err();
        let (summary, detail) = io_failure(&refused);
        assert_eq!((summary, detail.as_str()), (NOT_A_MAIL_SERVER, UNREADABLE));
        // What follows is never handed on either.
        server.write_all(b"* OK bonjour\r\n").await.unwrap();
        assert_eq!(
            guarded.read(&mut read).await.unwrap_err().to_string(),
            UNREADABLE
        );
        // Any other failure of the connection is said unreachable.
        let lost = io::Error::new(io::ErrorKind::InvalidData, "alerte TLS");
        assert_eq!(io_failure(&lost).0, UNREACHABLE);
    }

    /// Whether the parser reads `sent` whole, response after response.
    fn read_whole(sent: &[u8]) -> bool {
        let mut mirror = Mirror::default();
        mirror.take(sent) == Reading::Read && mirror.pending.is_empty()
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
