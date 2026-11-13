//! Reading a client's lines, bounded, for the simulated mail servers; and
//! an IMAP command whose strings may be literals (`{n}`, after the server's
//! go-ahead, or `{n+}`, RFC 7888), each given back quoted.

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The longest line a simulated server reads.
const MAX_LINE_BYTES: usize = 16 * 1024;

/// The next line without its ending; `None` once the client is gone or the
/// line is longer than a server reads.
pub(super) async fn next_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Option<String> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await.ok()?;
        if available.is_empty() {
            return None;
        }
        let (taken, done) = match available.iter().position(|b| *b == b'\n') {
            Some(end) => (end + 1, true),
            None => (available.len(), false),
        };
        line.extend_from_slice(&available[..taken]);
        reader.consume(taken);
        if line.len() > MAX_LINE_BYTES {
            return None;
        }
        if done {
            let text = String::from_utf8_lossy(&line);
            return Some(text.trim_end_matches(['\r', '\n']).to_string());
        }
    }
}

/// The next IMAP command, its literals read and quoted in their place.
pub(super) async fn next_command<S>(stream: &mut S) -> Option<String>
where
    S: AsyncBufRead + AsyncWrite + Unpin,
{
    let mut command = next_line(stream).await?;
    while let Some((before, size, waits)) = literal_at_end(&command) {
        if size > MAX_LINE_BYTES {
            return None;
        }
        if waits {
            stream.write_all(b"+ suite\r\n").await.ok()?;
        }
        let mut bytes = vec![0; size];
        stream.read_exact(&mut bytes).await.ok()?;
        let text = String::from_utf8_lossy(&bytes);
        let quoted = text.replace('\\', "\\\\").replace('"', "\\\"");
        let rest = next_line(stream).await?;
        command = format!("{before}\"{quoted}\"{rest}");
    }
    Some(command)
}

/// The command before a literal ending it, the literal's size, and whether
/// the client waits for the server's go-ahead.
fn literal_at_end(line: &str) -> Option<(String, usize, bool)> {
    let inner = line.strip_suffix('}')?;
    let open = inner.rfind('{')?;
    let (count, waits) = match inner[open + 1..].strip_suffix('+') {
        Some(count) => (count, false),
        None => (&inner[open + 1..], true),
    };
    let size = count.parse().ok()?;
    Some((line[..open].to_string(), size, waits))
}
