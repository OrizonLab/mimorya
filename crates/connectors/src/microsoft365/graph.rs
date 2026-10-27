//! Microsoft Graph as the tools read it (ADR-117): a `GET` under the
//! company's Graph address, with the access token of the person the run
//! serves, bounded in time and in size, never following a redirection
//! (the hardened client). Graph's own words never reach the model when it
//! refuses: only what its status means.

use std::time::Duration;

use reqwest_mcp::Client;
use reqwest_mcp::header::{ACCEPT, AUTHORIZATION, HeaderValue};
use serde_json::Value;
use url::Url;

use crate::outbound::{TOO_SLOW, UNREACHABLE};

/// The most of an answer read: a page of messages or events, or one
/// message's text, is far less.
pub const MAX_GRAPH_BYTES: usize = 1024 * 1024;

/// What a tool reads under the person's Graph address.
#[derive(Clone)]
pub struct Graph {
    base: Url,
    client: Client,
    timeout: Duration,
}

/// What Graph's refusal means, in French, for the model and the person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphFailure {
    /// 401: the token no longer opens the account.
    Unauthorized,
    /// 403: the permission was not granted.
    Forbidden,
    /// 404: nothing there, or nothing the person may read.
    NotFound,
    /// 429: too many requests.
    Throttled,
    Other(String),
}

impl GraphFailure {
    pub fn message(&self) -> String {
        match self {
            Self::Unauthorized => "Microsoft 365 a refusé l’accès au compte de la personne \
                 servie : réessayez ; si le refus persiste, la personne relie de nouveau son compte"
                .into(),
            Self::Forbidden => "Microsoft 365 refuse cette lecture : la permission \
                 correspondante n’a pas été accordée à l’application"
                .into(),
            Self::NotFound => "Introuvable dans votre compte Microsoft 365".into(),
            Self::Throttled => {
                "Microsoft 365 limite les demandes en ce moment : réessayez plus tard".into()
            }
            Self::Other(message) => message.clone(),
        }
    }
}

impl Graph {
    /// `base` is the stored Graph address, checked at the start.
    pub fn new(base: &str, client: Client, timeout: Duration) -> anyhow::Result<Self> {
        let mut base = Url::parse(base)?;
        // Segments are pushed after the version, never replacing it.
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        Ok(Self {
            base,
            client,
            timeout,
        })
    }

    /// The address of these path segments (each encoded whole, a `/` in
    /// an identifier included) and query.
    pub fn address(&self, segments: &[&str], query: &[(&str, String)]) -> Url {
        let mut url = self.base.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().extend(segments);
        }
        if !query.is_empty() {
            let pairs: Vec<String> = query
                .iter()
                .map(|(name, value)| format!("{}={}", encoded(name), encoded(value)))
                .collect();
            url.set_query(Some(&pairs.join("&")));
        }
        url
    }

    /// Graph's JSON answer to a `GET` of `url`, with the person's token and
    /// Graph's `Prefer` header, if any.
    pub async fn get(
        &self,
        token: &str,
        url: Url,
        prefer: Option<&str>,
    ) -> Result<Value, GraphFailure> {
        let mut bearer = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| GraphFailure::Unauthorized)?;
        bearer.set_sensitive(true);
        let mut request = self
            .client
            .get(url)
            .header(AUTHORIZATION, bearer)
            .header(ACCEPT, "application/json");
        if let Some(prefer) = prefer {
            request = request.header("Prefer", prefer);
        }
        let sent = tokio::time::timeout(self.timeout, async {
            let response = request.send().await?;
            let status = response.status().as_u16();
            let body = read_bounded(response).await?;
            Ok::<_, reqwest_mcp::Error>((status, body))
        })
        .await;
        let (status, body) = match sent {
            Err(_) => return Err(GraphFailure::Other(TOO_SLOW.into())),
            Ok(Err(_)) => return Err(GraphFailure::Other(UNREACHABLE.into())),
            Ok(Ok(answer)) => answer,
        };
        match status {
            200..=299 => {}
            401 => return Err(GraphFailure::Unauthorized),
            403 => return Err(GraphFailure::Forbidden),
            404 => return Err(GraphFailure::NotFound),
            429 => return Err(GraphFailure::Throttled),
            status => {
                return Err(GraphFailure::Other(format!(
                    "Microsoft 365 ne répond pas comme attendu (HTTP {status})"
                )));
            }
        }
        let body = body.ok_or_else(|| {
            GraphFailure::Other("Réponse de Microsoft 365 trop longue : précisez la demande".into())
        })?;
        serde_json::from_slice(&body)
            .map_err(|_| GraphFailure::Other("Réponse de Microsoft 365 illisible".into()))
    }
}

/// A query's name or value, its spaces as `%20` as Graph documents them
/// (a `+` is not read as a space everywhere): the form encoding writes a
/// `+` of the text as `%2B`, so each `+` it leaves is a space.
fn encoded(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

/// The body, or `None` past `MAX_GRAPH_BYTES`.
async fn read_bounded(
    mut response: reqwest_mcp::Response,
) -> Result<Option<Vec<u8>>, reqwest_mcp::Error> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > MAX_GRAPH_BYTES {
            return Ok(None);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Some(body))
}
