//! The events of an MCP server's answer to a message, read up to
//! `MAX_RESPONSE_BYTES` in all (ADR-161): the bytes up to the bound are
//! parsed, those beyond never read. The event being read at the bound is
//! dropped, and the platform's answer, if the message awaits one, ends the
//! stream instead; the cut is noted.

use std::fmt;
use std::future::ready;

use bytes::Bytes;
use futures::stream::{self, BoxStream};
use futures::{Stream, StreamExt};
use reqwest_mcp::Response;
use rmcp::model::ServerJsonRpcMessage;
use rmcp::transport::streamable_http_client::SseError;
use sse_stream::{Sse, SseStream};

use super::body::{Cuts, MAX_RESPONSE_BYTES};

/// The events of `response`, `MAX_RESPONSE_BYTES` of them in all at most:
/// beyond, the stream ends with `ending`, if any, the cut noted in `cuts`.
pub fn bounded_events(
    response: Response,
    ending: Option<ServerJsonRpcMessage>,
    cuts: Cuts,
) -> BoxStream<'static, Result<Sse, SseError>> {
    events_within(response.bytes_stream(), MAX_RESPONSE_BYTES, ending, cuts)
}

/// The same, of any stream of bytes.
fn events_within<S, E>(
    bytes: S,
    limit: usize,
    ending: Option<ServerJsonRpcMessage>,
    cuts: Cuts,
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
        .scan((ending, false), move |(ending, over), event| {
            if *over {
                return ready(None);
            }
            let event = match event {
                Err(SseError::Body(error)) if is_beyond(error.as_ref()) => {
                    *over = true;
                    cuts.record();
                    ending.take().map(Ok)
                }
                event => Some(event),
            };
            ready(event)
        })
        .boxed()
}

/// Where a stream of bytes stands against its bound.
enum Stage {
    Reading(usize),
    /// The bytes up to the bound given: `Beyond` comes next.
    AtBound,
    Done,
}

/// The bytes of `bytes` up to `limit` in all, the chunk crossing it cut at
/// it; then `Unread::Beyond`, and no more.
fn within<S, E>(bytes: S, limit: usize) -> impl Stream<Item = Result<Bytes, Unread>> + Send
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    let start = (bytes.boxed(), Stage::Reading(0));
    stream::unfold(start, move |(mut bytes, stage)| async move {
        let read = match stage {
            Stage::Reading(read) => read,
            Stage::AtBound => return Some((Err(Unread::Beyond), (bytes, Stage::Done))),
            Stage::Done => return None,
        };
        match bytes.next().await? {
            Ok(chunk) if chunk.len() <= limit - read => {
                let read = read + chunk.len();
                Some((Ok(chunk), (bytes, Stage::Reading(read))))
            }
            Ok(chunk) if read < limit => {
                let up_to_bound = chunk.slice(..limit - read);
                Some((Ok(up_to_bound), (bytes, Stage::AtBound)))
            }
            Ok(_) => Some((Err(Unread::Beyond), (bytes, Stage::Done))),
            Err(error) => Some((Err(Unread::Failed(Box::new(error))), (bytes, Stage::Done))),
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
            Unread::Beyond => f.write_str("bound reached"),
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
        ClientJsonRpcMessage, ClientRequest, JsonRpcMessage, NumberOrString, PingRequest,
    };

    use super::super::body::{TOO_LONG, too_long};
    use super::*;

    fn ping() -> ClientJsonRpcMessage {
        let request = ClientRequest::PingRequest(PingRequest::default());
        ClientJsonRpcMessage::request(request, NumberOrString::Number(7))
    }

    /// The events of `chunks` bounded at `limit` bytes, ended by the answer
    /// to a ping; whether a cut was noted.
    async fn read(chunks: &[&'static str], limit: usize) -> (Vec<Result<Sse, String>>, bool) {
        let chunks: Vec<Result<Bytes, std::io::Error>> = chunks
            .iter()
            .map(|c| Ok(Bytes::from_static(c.as_bytes())))
            .collect();
        let cuts = Cuts::default();
        let events = events_within(stream::iter(chunks), limit, too_long(&ping()), cuts.clone())
            .map(|event| event.map_err(|error| error.to_string()))
            .collect()
            .await;
        (events, cuts.any())
    }

    fn data(event: &Result<Sse, String>) -> String {
        event.as_ref().unwrap().data.clone().unwrap_or_default()
    }

    fn is_the_platform_s_answer(event: &Result<Sse, String>) -> bool {
        let answer: ServerJsonRpcMessage = serde_json::from_str(&data(event)).unwrap();
        let JsonRpcMessage::Error(error) = answer else {
            return false;
        };
        error.id == Some(NumberOrString::Number(7)) && error.error.message == TOO_LONG
    }

    #[tokio::test]
    async fn the_events_within_the_bound_pass_as_they_are() {
        let (events, cut) = read(&["data: un\n\n", "data: deux\n\n"], 22).await;
        let data: Vec<String> = events.iter().map(data).collect();
        assert_eq!(data, ["un", "deux"]);
        assert!(!cut);
    }

    #[tokio::test]
    async fn beyond_the_bound_the_platform_s_answer_ends_the_stream() {
        let chunks = ["data: un\n\n", "data: deux\n\n", "data: trois\n\n"];
        let (events, cut) = read(&chunks, 21).await;
        assert_eq!(events.len(), 2, "{events:?}");
        assert_eq!(data(&events[0]), "un");
        // The event being read is dropped: the answer to the ping instead.
        assert!(is_the_platform_s_answer(&events[1]), "{events:?}");
        assert!(cut);
    }

    #[tokio::test]
    async fn what_a_chunk_crossing_the_bound_holds_within_it_is_read() {
        // One chunk: its first event within the bound, its second across.
        let (events, _) = read(&["data: un\n\ndata: deux\n\n"], 21).await;
        assert_eq!(events.len(), 2, "{events:?}");
        assert_eq!(data(&events[0]), "un");
        assert!(is_the_platform_s_answer(&events[1]), "{events:?}");
    }

    #[tokio::test]
    async fn what_awaits_no_answer_gets_none() {
        let chunks = [Ok::<_, std::io::Error>(Bytes::from_static(b"data: un\n\n"))];
        let cuts = Cuts::default();
        let events: Vec<_> = events_within(stream::iter(chunks), 4, None, cuts.clone())
            .collect()
            .await;
        assert!(events.is_empty(), "{events:?}");
        assert!(cuts.any());
    }

    #[tokio::test]
    async fn a_failed_read_is_told_as_such() {
        let chunks = [
            Ok(Bytes::from_static(b"data: un\n\n")),
            Err(std::io::Error::other("coupure")),
        ];
        let cuts = Cuts::default();
        let events: Vec<_> =
            events_within(stream::iter(chunks), 1024, too_long(&ping()), cuts.clone())
                .collect()
                .await;
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(
            events[1]
                .as_ref()
                .is_err_and(|e| e.to_string().contains("coupure"))
        );
        assert!(!cuts.any());
    }
}
