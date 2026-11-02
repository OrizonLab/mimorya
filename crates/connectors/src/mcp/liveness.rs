//! Whether a kept session's server still answers (ADR-164). A tool call
//! that failed without an answer of the server's (none in time, a transport
//! refused or broken) is followed by a ping, in the background, one at a
//! time: a server that does not answer it within its bound has lost its
//! session, which then says it is closed, for the platform to start it
//! again. So are found a local server alive but mute, and an HTTP server
//! that refuses a session it no longer knows without saying 404 (the client
//! reaches a restarted server again only on a 404). A server that answers
//! the ping keeps its session, whatever its slow or failing tool.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rmcp::ServiceError;
use rmcp::model::{ClientRequest, PingRequest};

use super::bridge::McpClient;

/// A ping is answered at once: at most this long, or a call's delay if
/// shorter.
pub const PING_TIMEOUT: Duration = Duration::from_secs(5);

/// Shared by a session and its tools.
#[derive(Debug, Default)]
pub struct Liveness {
    /// The server did not answer a ping: the session is not kept.
    lost: AtomicBool,
    /// A ping is under way.
    checking: AtomicBool,
}

impl Liveness {
    /// Whether the server did not answer a ping.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    /// After a call that failed (`error`, or no answer in time: `None`):
    /// pings the server in the background, within `bound`, unless the
    /// server answered the call, its session is already lost, or a ping is
    /// under way.
    pub fn after_failure(
        self: &Arc<Self>,
        client: &Arc<McpClient>,
        error: Option<&ServiceError>,
        bound: Duration,
    ) {
        if error.is_some_and(answered) || self.is_lost() {
            return;
        }
        if self.checking.swap(true, Ordering::AcqRel) {
            return;
        }
        let (liveness, client) = (self.clone(), client.clone());
        tokio::spawn(async move {
            if !pinged(&client, bound).await {
                liveness.lost.store(true, Ordering::Release);
                tracing::warn!("An MCP server did not answer a ping after a failed call");
            }
            liveness.checking.store(false, Ordering::Release);
        });
    }
}

/// Whether the server answered the call itself: an error of its own, or an
/// answer the platform does not read (too long, ADR-161, or unexpected).
fn answered(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::McpError(_) | ServiceError::UnexpectedResponse
    )
}

/// Whether the server answered a ping within `bound`: an error of its own
/// (a server that does not know the method) is an answer too.
async fn pinged(client: &McpClient, bound: Duration) -> bool {
    let ping = client.send_request(ClientRequest::PingRequest(PingRequest::default()));
    match tokio::time::timeout(bound, ping).await {
        Ok(Ok(_)) => true,
        Ok(Err(error)) => answered(&error),
        Err(_) => false,
    }
}
