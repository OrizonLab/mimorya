//! Incremental parser for server-sent events (the Messages API stream).

/// One event of the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
}

impl SseParser {
    /// Feeds raw bytes; returns the events completed by them.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        // CR, LF and CRLF are all valid line endings: keep LF only.
        self.buffer
            .extend(chunk.iter().copied().filter(|b| *b != b'\r'));
        let mut events = Vec::new();
        while let Some(end) = self.buffer.windows(2).position(|w| w == b"\n\n") {
            let block: Vec<u8> = self.buffer.drain(..end + 2).collect();
            if let Some(event) = parse_block(&String::from_utf8_lossy(&block)) {
                events.push(event);
            }
        }
        events
    }
}

fn parse_block(block: &str) -> Option<SseEvent> {
    let mut event = String::from("message");
    let mut data: Vec<&str> = Vec::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    (!data.is_empty()).then(|| SseEvent {
        event,
        data: data.join("\n"),
    })
}
