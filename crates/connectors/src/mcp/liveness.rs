//! Whether a kept session's server still answers (ADR-164). A tool call
//! that failed without an answer of the server's (none in time, a transport
//! refused or broken) is followed by a ping, in the background, one at a
//! time, given as long as a call; one the server answered, even with more
//! `input_required` rounds than a call plays, is not. A server that does not
//! answer the ping has lost its session, which then says it is closed, for
//! the platform to start it again. So are found a local server alive but
//! mute, and an HTTP server that refuses a session it no longer knows
//! without saying 404 (the client reaches a restarted server again only on
//! a 404). A server that answers the ping keeps its session, even refusing
//! it: with an error of its own, or a 429 (alive, limiting its rate).
//! Limit: a server that handles one request at a time answers the ping
//! only once its slow call is over: a call of more than twice the call
//! delay loses it its session.
//!
//! When the session ended is kept (ADR-165): lost, when the call left
//! unanswered was made; closed, as a watch every `WATCH_EVERY` finds it, the
//! watch ending with the session.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rmcp::ServiceError;
use rmcp::model::{ClientRequest, PingRequest};
use tokio::time::Instant;

use super::bridge::McpClient;
use super::failure::refused_with;

/// How often a session's transport is looked at, to date its closing.
pub const WATCH_EVERY: Duration = Duration::from_secs(5);

/// Shared by a session and its tools.
#[derive(Debug)]
pub struct Liveness {
    /// The server's prefix, what the logs name it by.
    server: String,
    /// The server did not answer a ping: the session is not kept.
    lost: AtomicBool,
    /// A ping is under way.
    checking: AtomicBool,
    /// When the session ended, once known.
    ended: OnceLock<Instant>,
}

impl Liveness {
    pub fn new(server: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            lost: AtomicBool::new(false),
            checking: AtomicBool::new(false),
            ended: OnceLock::new(),
        }
    }

    /// Whether the server did not answer a ping.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    /// When the session ended (lost, or its transport closed), once known.
    pub fn ended_at(&self) -> Option<Instant> {
        self.ended.get().copied()
    }

    /// Dates the closing of `client`'s transport, looking every
    /// `WATCH_EVERY`; ends with the session (once nothing holds its client).
    pub fn watch(self: &Arc<Self>, client: &Arc<McpClient>) {
        let (liveness, client) = (Arc::downgrade(self), Arc::downgrade(client));
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(WATCH_EVERY).await;
                let (Some(liveness), Some(client)) = (liveness.upgrade(), client.upgrade()) else {
                    return;
                };
                if client.is_transport_closed() {
                    let _ = liveness.ended.set(Instant::now());
                    return;
                }
                if liveness.ended_at().is_some() {
                    return;
                }
            }
        });
    }

    /// After a call made at `asked` that failed (`error`, or no answer in
    /// time: `None`): pings the server in the background, within `bound`,
    /// unless the server answered the call, its session is closed or lost
    /// already, or a ping is under way. A server lost then ended at `asked`.
    pub fn after_failure(
        self: &Arc<Self>,
        client: &Arc<McpClient>,
        error: Option<&ServiceError>,
        bound: Duration,
        asked: Instant,
    ) {
        if error.is_some_and(answered) || self.is_lost() || client.is_transport_closed() {
            return;
        }
        if self.checking.swap(true, Ordering::AcqRel) {
            return;
        }
        let (liveness, client) = (self.clone(), client.clone());
        tokio::spawn(async move {
            if !pinged(&client, bound).await {
                // Ended when the call the ping followed was made, as far as
                // the platform knows.
                let _ = liveness.ended.set(asked);
                liveness.lost.store(true, Ordering::Release);
                tracing::warn!(
                    server = %liveness.server,
                    "An MCP server did not answer a ping after a failed call"
                );
            }
            liveness.checking.store(false, Ordering::Release);
        });
    }
}

/// Whether the server answered the message itself: an error of its own, an
/// answer the platform does not read (too long, ADR-161, or unexpected),
/// more `input_required` rounds than a call plays, or a refusal of its rate
/// (429).
fn answered(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::McpError(_)
            | ServiceError::UnexpectedResponse
            | ServiceError::InputRequiredRoundsExceeded { .. }
    ) || refused_with(error, 429)
}

/// Whether the server answered a ping within `bound`.
async fn pinged(client: &McpClient, bound: Duration) -> bool {
    let ping = client.send_request(ClientRequest::PingRequest(PingRequest::default()));
    match tokio::time::timeout(bound, ping).await {
        Ok(Ok(_)) => true,
        Ok(Err(error)) => answered(&error),
        Err(_) => false,
    }
}
