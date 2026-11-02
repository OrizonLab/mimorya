//! What is read of an MCP server's answer (ADR-161): its JSON or its event
//! stream (`events`) up to `MAX_RESPONSE_BYTES`, a refusal up to
//! `MAX_ERROR_BYTES`, nothing beyond. A request whose answer was longer is
//! answered by the platform instead, with an error saying so (`TOO_LONG`);
//! a notification or a reply awaits no answer, and gets none. Every answer
//! cut is noted (`Cuts`): the MCP library does not say why a start failed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use reqwest_mcp::Response;
use rmcp::model::{ClientJsonRpcMessage, ErrorData, ServerJsonRpcMessage};

/// What is read of a server's answer to a message, JSON or event stream:
/// what its tools return reaches the model shortened anyway.
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// What is read of a refusal: enough for a JSON-RPC error, and far more
/// than the 200 characters of it an administrator reads.
pub const MAX_ERROR_BYTES: usize = 64 * 1024;

/// The platform's answer to a request whose server answered longer.
pub const TOO_LONG: &str = "La réponse du serveur MCP dépasse 4 Mio : elle n’a pas été lue au-delà";

/// Whether a client cut a server's answer to a message, since it was made.
#[derive(Clone, Default)]
pub struct Cuts(Arc<AtomicBool>);

impl Cuts {
    pub(super) fn record(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn any(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A body, read up to a bound.
pub enum Read {
    Whole(Vec<u8>),
    /// Its first bytes, up to the bound: the rest was not read.
    Cut(Vec<u8>),
}

/// The body of `response`, `limit` bytes of it at most.
pub async fn read_within(mut response: Response, limit: usize) -> Result<Read, reqwest_mcp::Error> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let room = limit - body.len();
        if chunk.len() > room {
            body.extend_from_slice(&chunk[..room]);
            return Ok(Read::Cut(body));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Read::Whole(body))
}

/// The platform's answer to `message` when its server's was too long: an
/// error, for a request; none, for what awaits no answer.
pub fn too_long(message: &ClientJsonRpcMessage) -> Option<ServerJsonRpcMessage> {
    let ClientJsonRpcMessage::Request(request) = message else {
        return None;
    };
    let error = ErrorData::internal_error(TOO_LONG, None);
    Some(ServerJsonRpcMessage::error(error, Some(request.id.clone())))
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use axum::http;
    use bytes::Bytes;
    use futures::stream;

    use super::*;

    /// `chunks`, as a server's answer sends them, read within `limit`.
    async fn read(chunks: &[&'static str], limit: usize) -> (bool, String) {
        let chunks: Vec<Result<Bytes, Infallible>> = chunks
            .iter()
            .map(|chunk| Ok(Bytes::from_static(chunk.as_bytes())))
            .collect();
        let body = reqwest_mcp::Body::wrap_stream(stream::iter(chunks));
        let response = Response::from(http::Response::new(body));
        match read_within(response, limit).await.unwrap() {
            Read::Whole(body) => (true, String::from_utf8(body).unwrap()),
            Read::Cut(body) => (false, String::from_utf8(body).unwrap()),
        }
    }

    #[tokio::test]
    async fn a_body_within_its_bound_is_read_whole() {
        assert_eq!(read(&["abc", "def"], 6).await, (true, "abcdef".into()));
        assert_eq!(read(&[], 6).await, (true, String::new()));
    }

    #[tokio::test]
    async fn a_body_beyond_its_bound_is_read_up_to_it() {
        // Cut within a chunk, or at its end, the next one unread.
        assert_eq!(read(&["abc", "def"], 5).await, (false, "abcde".into()));
        assert_eq!(read(&["abc", "def"], 3).await, (false, "abc".into()));
        assert_eq!(read(&["abc"], 0).await, (false, String::new()));
    }

    #[test]
    fn what_awaits_no_answer_gets_none() {
        let notification = ClientJsonRpcMessage::notification(
            rmcp::model::ClientNotification::InitializedNotification(Default::default()),
        );
        assert!(too_long(&notification).is_none());
    }

    #[test]
    fn the_bound_is_said_as_it_is() {
        assert!(TOO_LONG.contains(&format!("{} Mio", MAX_RESPONSE_BYTES >> 20)));
    }

    #[test]
    fn a_cut_is_noted_for_every_clone() {
        let cuts = Cuts::default();
        let kept = cuts.clone();
        assert!(!kept.any());
        cuts.record();
        assert!(kept.any());
    }
}
