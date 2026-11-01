//! What is read of an MCP server's answer (ADR-161): its JSON or its event
//! stream up to `MAX_RESPONSE_BYTES`, an error up to `MAX_ERROR_BYTES`,
//! nothing beyond. A request whose answer was longer is answered by the
//! platform instead, with an error saying so (`TOO_LONG`); a notification
//! or a reply awaits no answer, and gets none.

use std::fmt;
use std::future::ready;

use bytes::Bytes;
use futures::stream::{self, BoxStream};
use futures::{Stream, StreamExt};
use reqwest_mcp::Response;
use rmcp::model::{ClientJsonRpcMessage, ErrorData, ServerJsonRpcMessage};
use rmcp::transport::streamable_http_client::SseError;
use sse_stream::{Sse, SseStream};

/// What is read of a server's answer to a message, JSON or event stream:
/// what its tools return reaches the model shortened anyway.
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// What is read of an error: enough for a JSON-RPC error, and far more than
/// the 200 characters of it an administrator reads.
pub const MAX_ERROR_BYTES: usize = 64 * 1024;

/// The platform's answer to a request whose server answered longer.
pub const TOO_LONG: &str = "La réponse du serveur MCP dépasse 4 Mio : elle n’a pas été lue au-delà";

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

/// The events of `response`, `MAX_RESPONSE_BYTES` of them in all at most:
/// beyond, the stream ends with `ending`, if any, the event being read
/// dropped.
pub fn bounded_events(
    response: Response,
    ending: Option<ServerJsonRpcMessage>,
) -> BoxStream<'static, Result<Sse, SseError>> {
    events_within(response.bytes_stream(), MAX_RESPONSE_BYTES, ending)
}

/// The same, of any stream of bytes.
fn events_within<S, E>(
    bytes: S,
    limit: usize,
    ending: Option<ServerJsonRpcMessage>,
) -> BoxStream<'static, Result<Sse, SseError>>
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    let ending = ending.and_then(|answer| serde_json::to_string(&answer).ok());
    let ending = ending.map(|data| Sse {
        data: Some(data),
        ..Sse::default()
    });
    SseStream::from_bytes_stream(within(bytes, limit))
        .scan((ending, false), |(ending, over), event| {
            if *over {
                return ready(None);
            }
            let event = match event {
                Err(SseError::Body(error)) if is_beyond(error.as_ref()) => {
                    *over = true;
                    ending.take().map(Ok)
                }
                event => Some(event),
            };
            ready(event)
        })
        .boxed()
}

/// The bytes of `bytes` up to `limit` in all; past it, `Unread`, then no more.
fn within<S, E>(bytes: S, limit: usize) -> impl Stream<Item = Result<Bytes, Unread>> + Send
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    let start = (bytes.boxed(), 0usize, false);
    stream::unfold(start, move |(mut bytes, read, over)| async move {
        if over {
            return None;
        }
        match bytes.next().await? {
            Ok(chunk) if read + chunk.len() <= limit => {
                let read = read + chunk.len();
                Some((Ok(chunk), (bytes, read, false)))
            }
            Ok(_) => Some((Err(Unread::Beyond), (bytes, read, true))),
            Err(error) => Some((Err(Unread::Failed(Box::new(error))), (bytes, read, true))),
        }
    })
}

fn is_beyond(error: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    matches!(error.downcast_ref(), Some(Unread::Beyond))
}

/// Why a stream's bytes stopped: its bound reached, or its reading failed.
#[derive(Debug)]
enum Unread {
    Beyond,
    Failed(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for Unread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unread::Beyond => f.write_str(TOO_LONG),
            Unread::Failed(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Unread {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Unread::Beyond => None,
            Unread::Failed(error) => Some(error.as_ref()),
        }
    }
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        ClientNotification, ClientRequest, JsonRpcMessage, NumberOrString, PingRequest,
    };

    use super::*;

    fn ping() -> ClientJsonRpcMessage {
        let request = ClientRequest::PingRequest(PingRequest::default());
        ClientJsonRpcMessage::request(request, NumberOrString::Number(7))
    }

    /// The events of `chunks`, bounded at `limit` bytes, ended by the
    /// answer to a ping.
    async fn read(chunks: &[&'static str], limit: usize) -> Vec<Result<Sse, String>> {
        let chunks: Vec<Result<Bytes, std::io::Error>> = chunks
            .iter()
            .map(|c| Ok(Bytes::from_static(c.as_bytes())))
            .collect();
        events_within(stream::iter(chunks), limit, too_long(&ping()))
            .map(|event| event.map_err(|error| error.to_string()))
            .collect()
            .await
    }

    fn data(event: &Result<Sse, String>) -> String {
        event.as_ref().unwrap().data.clone().unwrap_or_default()
    }

    #[tokio::test]
    async fn the_events_within_the_bound_pass_as_they_are() {
        let events = read(&["data: un\n\n", "data: deux\n\n"], 22).await;
        let data: Vec<String> = events.iter().map(data).collect();
        assert_eq!(data, ["un", "deux"]);
    }

    #[tokio::test]
    async fn beyond_the_bound_the_platform_s_answer_ends_the_stream() {
        let events = read(&["data: un\n\n", "data: deux\n\n", "data: trois\n\n"], 21).await;
        assert_eq!(events.len(), 2, "{events:?}");
        assert_eq!(data(&events[0]), "un");
        // The event being read is dropped: the answer to the ping instead.
        let answer: ServerJsonRpcMessage = serde_json::from_str(&data(&events[1])).unwrap();
        let JsonRpcMessage::Error(error) = answer else {
            panic!("an error: {answer:?}");
        };
        assert_eq!(error.id, Some(NumberOrString::Number(7)));
        assert_eq!(error.error.message, TOO_LONG);
    }

    #[tokio::test]
    async fn what_awaits_no_answer_gets_none() {
        let chunks = [Ok::<_, std::io::Error>(Bytes::from_static(b"data: un\n\n"))];
        let notification = ClientJsonRpcMessage::notification(
            ClientNotification::InitializedNotification(Default::default()),
        );
        assert!(too_long(&notification).is_none());
        let events: Vec<_> = events_within(stream::iter(chunks), 4, None).collect().await;
        assert!(events.is_empty(), "{events:?}");
    }

    #[tokio::test]
    async fn a_failed_read_is_told_as_such() {
        let chunks = [
            Ok(Bytes::from_static(b"data: un\n\n")),
            Err(std::io::Error::other("coupure")),
        ];
        let events: Vec<_> = events_within(stream::iter(chunks), 1024, too_long(&ping()))
            .collect()
            .await;
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(
            events[1]
                .as_ref()
                .is_err_and(|e| e.to_string().contains("coupure"))
        );
    }

    #[test]
    fn the_bound_is_said_as_it_is() {
        assert!(TOO_LONG.contains(&format!("{} Mio", MAX_RESPONSE_BYTES >> 20)));
    }
}
