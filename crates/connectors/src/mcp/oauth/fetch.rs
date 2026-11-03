//! The requests the discovery makes (ADR-171), through the hardened client
//! (public addresses only unless allowed, no redirect, the operator's
//! proxy): each bounded in time, a document in size; the server asked to
//! start without a token is read for its status and challenge only, its
//! body, which may be a stream that never ends, left unread. A failure is
//! said in French with the address quoted short, never in the client's own
//! words, which write the whole address; a server that says to come back
//! later is told apart from one that has nothing there.

use std::future::Future;
use std::time::Duration;

use reqwest_mcp::header::{ACCEPT, CONTENT_TYPE, WWW_AUTHENTICATE};
use reqwest_mcp::{Client, RequestBuilder, Response};

use super::quoted::quoted;
use crate::mcp::http::{Read, read_within};
use crate::outbound::{
    NetworkPolicy, Redactor, TOO_SLOW, explained, http_client, request_cause, url_problem,
};

/// Metadata documents are a few kilobytes.
pub(super) const MAX_DOCUMENT_BYTES: usize = 256 * 1024;
/// What starts an MCP session: the server answers it, or says it wants a
/// token.
const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"mimorya","version":"1"}}}"#;

/// How the server answered a request without a token.
pub(super) struct Probe {
    pub status: u16,
    /// Every `WWW-Authenticate` line, joined.
    pub challenge: Option<String>,
}

pub(super) struct Fetcher {
    client: Client,
    policy: NetworkPolicy,
    timeout: Duration,
}

impl Fetcher {
    pub fn new(policy: NetworkPolicy, timeout: Duration) -> Result<Self, String> {
        let client = http_client(policy).map_err(|error| error.to_string())?;
        Ok(Self {
            client,
            policy,
            timeout,
        })
    }

    /// The MCP server at `url` asked to start a session without a token.
    pub async fn probe(&self, url: &str) -> Result<Probe, String> {
        let request = self
            .client
            .post(url)
            .header(ACCEPT, "application/json, text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .body(INITIALIZE);
        let response = self.within(url, self.send(request, url)).await?;
        let lines: Vec<&str> = response
            .headers()
            .get_all(WWW_AUTHENTICATE)
            .iter()
            .filter_map(|line| line.to_str().ok())
            .collect();
        let probe = Probe {
            status: response.status().as_u16(),
            challenge: (!lines.is_empty()).then(|| lines.join(", ")),
        };
        match unavailable(url, probe.status) {
            Some(problem) => Err(problem),
            None => Ok(probe),
        }
    }

    /// The document at `location`, within its bounds; `None` where there is
    /// none (any answer but 200 that does not say to come back later).
    pub async fn document(&self, location: &str) -> Result<Option<Vec<u8>>, String> {
        let request = self.client.get(location).header(ACCEPT, "application/json");
        self.within(location, async {
            let response = self.send(request, location).await?;
            let status = response.status().as_u16();
            if let Some(problem) = unavailable(location, status) {
                return Err(problem);
            }
            if status != 200 {
                return Ok(None);
            }
            match read_within(response, MAX_DOCUMENT_BYTES).await {
                Ok(Read::Whole(body)) => Ok(Some(body)),
                Ok(Read::Cut(_)) => Err(format!(
                    "Réponse de « {} » trop longue ({} Kio au plus)",
                    quoted(location),
                    MAX_DOCUMENT_BYTES / 1024
                )),
                Err(error) => Err(failed(location, error)),
            }
        })
        .await
    }

    /// `request`, sent to `address` if the network rules allow it.
    async fn send(&self, request: RequestBuilder, address: &str) -> Result<Response, String> {
        if let Some(problem) = url_problem(address, self.policy) {
            return Err(format!("« {} » : {problem}", quoted(address)));
        }
        request.send().await.map_err(|error| failed(address, error))
    }

    /// `work`, given at most the time of one request.
    async fn within<T>(
        &self,
        address: &str,
        work: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        tokio::time::timeout(self.timeout, work)
            .await
            .map_err(|_| format!("« {} » : {TOO_SLOW}", quoted(address)))?
    }
}

/// A request that got no answer, said as the connectors say it: refused
/// address, timeout or unreachable server, with a short detail that never
/// holds the address.
fn failed(address: &str, error: reqwest_mcp::Error) -> String {
    let cause = request_cause(&error.without_url());
    format!(
        "« {} » : {}",
        quoted(address),
        explained(cause, &Redactor::default())
    )
}

/// The server's own trouble, if its answer says so: not an answer about
/// its authorization.
fn unavailable(address: &str, status: u16) -> Option<String> {
    (status == 429 || status >= 500).then(|| {
        format!(
            "« {} » a répondu {status} : réessayez plus tard",
            quoted(address)
        )
    })
}
