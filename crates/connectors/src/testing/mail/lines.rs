//! Reading a client's lines, bounded, for the simulated mail servers.

use tokio::io::{AsyncBufRead, AsyncBufReadExt};

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
