//! What is said to an SMTP server in clear before STARTTLS (ADR-205): its
//! greeting, `EHLO`, its extensions, `STARTTLS` and its go-ahead, nothing
//! else. Answers are read byte after byte, so that nothing the server (or
//! someone on the path) sends after the go-ahead is taken for an answer: it
//! reaches the TLS handshake, which fails. A server offering no STARTTLS
//! hears `QUIT` and no credential.

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{HELLO_NAME, NO_STARTTLS, NOT_A_MAIL_SERVER};
use crate::outbound::{Cause, Connection, TOO_LONG, UNREACHABLE};

/// A reply line's longest form, ending included (RFC 5321 says 512).
const MAX_LINE_BYTES: usize = 1000;
/// The most lines a reply holds.
const MAX_LINES: usize = 100;

/// A reply: its code and its lines' texts.
struct Reply {
    code: u16,
    lines: Vec<String>,
}

/// The connection, the server having said yes to STARTTLS: TLS is next.
pub(super) async fn start_tls(connection: &mut Connection) -> Result<(), Cause> {
    expect(reply(connection).await?, 220, "accueil")?;
    say(connection, &format!("EHLO {HELLO_NAME}")).await?;
    let extensions = expect(reply(connection).await?, 250, "EHLO")?;
    let offers = extensions.lines.iter().skip(1).any(|line| {
        line.split_whitespace()
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("STARTTLS"))
    });
    if !offers {
        // Left as SMTP leaves: its goodbye awaited, whatever it says.
        if say(connection, "QUIT").await.is_ok() {
            let _ = reply(connection).await;
        }
        return Err(Cause {
            summary: NO_STARTTLS,
            detail: String::new(),
        });
    }
    say(connection, "STARTTLS").await?;
    expect(reply(connection).await?, 220, "STARTTLS").map(|_| ())
}

fn expect(reply: Reply, code: u16, after: &str) -> Result<Reply, Cause> {
    if reply.code == code {
        return Ok(reply);
    }
    Err(Cause {
        summary: NOT_A_MAIL_SERVER,
        detail: format!("réponse {} après {after}", reply.code),
    })
}

async fn say(connection: &mut Connection, command: &str) -> Result<(), Cause> {
    let written = async {
        connection
            .write_all(format!("{command}\r\n").as_bytes())
            .await?;
        connection.flush().await
    };
    written.await.map_err(|error| Cause {
        summary: UNREACHABLE,
        detail: error.to_string(),
    })
}

/// A whole reply: lines of `NNN-` then one of `NNN ` (or `NNN` alone).
async fn reply(connection: &mut Connection) -> Result<Reply, Cause> {
    let mut lines = Vec::new();
    loop {
        let line = line(connection).await?;
        let code = line
            .get(..3)
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| unreadable("réponse sans code"))?;
        let more = line.as_bytes().get(3) == Some(&b'-');
        lines.push(line.get(4..).unwrap_or_default().to_string());
        if !more {
            return Ok(Reply { code, lines });
        }
        if lines.len() >= MAX_LINES {
            return Err(Cause {
                summary: TOO_LONG,
                detail: format!("réponse de plus de {MAX_LINES} lignes"),
            });
        }
    }
}

/// One line, byte after byte, without its ending.
async fn line(connection: &mut Connection) -> Result<String, Cause> {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n") {
        if bytes.len() >= MAX_LINE_BYTES {
            return Err(Cause {
                summary: TOO_LONG,
                detail: format!("ligne de plus de {MAX_LINE_BYTES} octets"),
            });
        }
        let byte = connection.read_u8().await.map_err(|error| Cause {
            summary: UNREACHABLE,
            detail: error.to_string(),
        })?;
        bytes.push(byte);
    }
    bytes.truncate(bytes.len() - 2);
    String::from_utf8(bytes).map_err(|_| unreadable("ligne illisible"))
}

fn unreadable(detail: &str) -> Cause {
    Cause {
        summary: NOT_A_MAIL_SERVER,
        detail: detail.into(),
    }
}
