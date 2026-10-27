//! A provider's API as the tools of a linked account reach it: Microsoft
//! Graph (ADR-117), Gmail and Google Calendar (ADR-122). A `GET` under the
//! company's declared address, or a `POST` of what a person validated
//! (ADR-119), with the access token of the person the run serves, bounded
//! in time and in size, never following a redirection (the hardened
//! client), never repeated. The provider's own words never reach the model
//! when it refuses: only what its status means.

use std::time::Duration;

use mimorya_core::account_links::LinkKind;
use reqwest_mcp::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use reqwest_mcp::{Client, RequestBuilder};
use serde_json::Value;
use url::Url;

use crate::outbound::{TOO_SLOW, UNREACHABLE};

/// The most of an answer read: a page of messages or events, or one
/// message's text, is far less.
pub const MAX_ANSWER_BYTES: usize = 1024 * 1024;

/// What a tool reads under the person's address at a provider.
#[derive(Clone)]
pub struct ProviderApi {
    kind: LinkKind,
    base: Url,
    client: Client,
    timeout: Duration,
}

/// What a provider's refusal means, in French, for the model and the
/// person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiFailure {
    /// 401: the token no longer opens the account.
    Unauthorized,
    /// 403: the permission was not granted.
    Forbidden,
    /// 404: nothing there, or nothing the person may read.
    NotFound,
    /// 429: too many requests.
    Throttled,
    /// Another status.
    Failed(u16),
    /// No answer: too slow, or unreachable.
    Unanswered(&'static str),
    /// An answer not read: too long, or unreadable.
    Other(String),
}

impl ApiFailure {
    /// Whether the refusal says the provider did nothing: a request it
    /// answered with a client error. Unanswered, or failing on its side,
    /// it may have acted.
    pub fn settled(&self) -> bool {
        match self {
            Self::Failed(status) => *status < 500,
            Self::Unanswered(_) => false,
            _ => true,
        }
    }

    /// What the refusal of this kind's provider means.
    pub fn message(&self, kind: LinkKind) -> String {
        let product = kind.product();
        match self {
            Self::Unauthorized => format!(
                "{product} a refusé l’accès au compte de la personne servie : réessayez ; si le \
                 refus persiste, la personne relie de nouveau son compte dans « Mon compte »"
            ),
            // Google also refuses so past its limits of requests.
            Self::Forbidden => match kind {
                LinkKind::Microsoft365 => format!(
                    "{product} refuse cette demande : la permission correspondante n’a pas été \
                     accordée à l’application"
                ),
                LinkKind::GoogleWorkspace => format!(
                    "{product} refuse cette demande : la permission correspondante n’a pas été \
                     accordée à l’application, ou ses limites de demandes sont atteintes ; \
                     réessayez plus tard"
                ),
            },
            Self::NotFound => format!("Introuvable dans votre compte {product}"),
            Self::Throttled => {
                format!("{product} limite les demandes en ce moment : réessayez plus tard")
            }
            Self::Failed(status) => {
                format!("{product} ne répond pas comme attendu (HTTP {status})")
            }
            Self::Unanswered(why) => (*why).into(),
            Self::Other(message) => message.clone(),
        }
    }
}

impl ProviderApi {
    /// `base` is the stored address of the provider's API, checked at the
    /// start.
    pub fn new(
        kind: LinkKind,
        base: &str,
        client: Client,
        timeout: Duration,
    ) -> anyhow::Result<Self> {
        let mut base = Url::parse(base)?;
        // Segments are pushed after the version, never replacing it.
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        Ok(Self {
            kind,
            base,
            client,
            timeout,
        })
    }

    /// The provider whose API this is.
    pub fn kind(&self) -> LinkKind {
        self.kind
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

    /// The JSON answer to a `GET` of `url`, with the person's token and
    /// Graph's `Prefer` header, if any.
    pub async fn get(
        &self,
        token: &str,
        url: Url,
        prefer: Option<&str>,
    ) -> Result<Value, ApiFailure> {
        let body = self.send(token, self.client.get(url), prefer).await?;
        let body = body.ok_or_else(|| {
            ApiFailure::Other(format!(
                "Réponse de {} trop longue : précisez la demande",
                self.kind.product()
            ))
        })?;
        serde_json::from_slice(&body).map_err(|_| self.unreadable())
    }

    /// The JSON answer to a `POST` of `body` to `url`, sent once: done
    /// once the provider accepts it, its answer `None` when there is none
    /// to read (a message accepted for sending).
    pub async fn post(
        &self,
        token: &str,
        url: Url,
        body: &Value,
        prefer: Option<&str>,
    ) -> Result<Option<Value>, ApiFailure> {
        let body = serde_json::to_vec(body).map_err(|_| self.unreadable())?;
        let request = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .body(body);
        let body = self.send(token, request, prefer).await?;
        Ok(body.and_then(|body| serde_json::from_slice(&body).ok()))
    }

    fn unreadable(&self) -> ApiFailure {
        ApiFailure::Other(format!("Réponse de {} illisible", self.kind.product()))
    }

    /// The request sent with the person's token, its answer read within
    /// the delay: its body once accepted, `None` past `MAX_ANSWER_BYTES`.
    async fn send(
        &self,
        token: &str,
        request: RequestBuilder,
        prefer: Option<&str>,
    ) -> Result<Option<Vec<u8>>, ApiFailure> {
        let mut bearer = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| ApiFailure::Unauthorized)?;
        bearer.set_sensitive(true);
        let mut request = request
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
            Err(_) => return Err(ApiFailure::Unanswered(TOO_SLOW)),
            Ok(Err(_)) => return Err(ApiFailure::Unanswered(UNREACHABLE)),
            Ok(Ok(answer)) => answer,
        };
        match status {
            200..=299 => {}
            401 => return Err(ApiFailure::Unauthorized),
            403 => return Err(ApiFailure::Forbidden),
            404 => return Err(ApiFailure::NotFound),
            429 => return Err(ApiFailure::Throttled),
            status => return Err(ApiFailure::Failed(status)),
        }
        Ok(body)
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

/// The body, or `None` past `MAX_ANSWER_BYTES`.
async fn read_bounded(
    mut response: reqwest_mcp::Response,
) -> Result<Option<Vec<u8>>, reqwest_mcp::Error> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > MAX_ANSWER_BYTES {
            return Ok(None);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Some(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_client_error_says_the_provider_did_nothing() {
        for settled in [
            ApiFailure::Unauthorized,
            ApiFailure::Forbidden,
            ApiFailure::Throttled,
            ApiFailure::Failed(400),
        ] {
            assert!(settled.settled(), "{settled:?}");
        }
        for unsettled in [
            ApiFailure::Failed(503),
            ApiFailure::Unanswered(TOO_SLOW),
            ApiFailure::Unanswered(UNREACHABLE),
        ] {
            assert!(!unsettled.settled(), "{unsettled:?}");
        }
    }

    #[test]
    fn a_refusal_names_its_provider() {
        let microsoft = ApiFailure::NotFound.message(LinkKind::Microsoft365);
        assert_eq!(microsoft, "Introuvable dans votre compte Microsoft 365");
        let google = ApiFailure::Forbidden.message(LinkKind::GoogleWorkspace);
        assert!(google.starts_with("Google Workspace refuse"), "{google}");
        assert!(google.contains("limites"), "{google}");
    }
}
