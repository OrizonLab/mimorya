//! Microsoft as a company's Teams bot reaches it (ADR-138), one request at
//! a time, bounded in time and in size, never following a redirection (the
//! hardened client), never repeated: Microsoft Entra gives the bot's
//! application a token for the Bot Framework from its client secret
//! (client credentials), kept until shortly before it expires, for that
//! secret only; with it, the bot writes in a conversation of the service an
//! activity named, and asks who its member is. Only a service Microsoft's
//! Teams (`ServiceUrls`) is written to. No address, token nor secret is
//! ever printed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mimorya_core::gateways::teams::{BOT_FRAMEWORK_SCOPE, ServiceUrls, TeamsApplication};
use reqwest_mcp::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest_mcp::{Client, RequestBuilder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use url::Url;
use url::form_urlencoded::Serializer;

use super::keys::KeyCache;
use super::{TeamsEndpoints, TeamsFailure, TeamsMemberProfile, refusal_cause};
use crate::outbound::{TOO_SLOW, UNREACHABLE};

/// The most of an answer read: Microsoft's are a few kibibytes.
const MAX_ANSWER_BYTES: usize = 256 * 1024;
/// How long Microsoft has to answer.
const TIMEOUT: Duration = Duration::from_secs(10);
/// How long before it expires a token is no longer used.
const EXPIRY_MARGIN: Duration = Duration::from_secs(5 * 60);

/// Microsoft, as the bot reaches it, with the hardened client.
#[derive(Clone)]
pub struct TeamsApi {
    inner: Arc<Inner>,
}

struct Inner {
    login: Url,
    openid: Url,
    services: ServiceUrls,
    client: Client,
    /// Each application's token, with its secret's digest and expiry.
    tokens: Mutex<HashMap<String, CachedToken>>,
    keys: Mutex<Option<KeyCache>>,
    /// Held while the keys are read.
    reading: tokio::sync::Mutex<()>,
}

struct CachedToken {
    secret: [u8; 32],
    token: String,
    usable_until: Instant,
}

impl TeamsApi {
    pub fn new(endpoints: TeamsEndpoints, client: Client) -> anyhow::Result<Self> {
        Ok(Self {
            inner: Arc::new(Inner {
                login: Url::parse(&endpoints.login)?,
                openid: Url::parse(&endpoints.openid)?,
                services: endpoints.services,
                client,
                tokens: Mutex::new(HashMap::new()),
                keys: Mutex::new(None),
                reading: tokio::sync::Mutex::new(()),
            }),
        })
    }

    /// The services activities may name.
    pub fn services(&self) -> &ServiceUrls {
        &self.inner.services
    }

    /// Whether Microsoft Entra gives `app` a token now, with its secret: a
    /// declaration is checked so.
    pub async fn try_application(&self, app: &TeamsApplication) -> Result<(), TeamsFailure> {
        self.new_token(app).await.map(drop)
    }

    /// Writes `text` (Teams' Markdown) in the conversation `conversation` of
    /// the service `service_url`, as the bot `app`.
    pub async fn send(
        &self,
        app: &TeamsApplication,
        to: (&str, &str),
        text: &str,
    ) -> Result<(), TeamsFailure> {
        let body = json!({ "type": "message", "text": text, "textFormat": "markdown" });
        self.write_activity(app, to, None, &body).await.map(drop)
    }

    /// Writes the activity `body` in the conversation `conversation` of the
    /// service `service_url`, as the bot `app`: a new one, or in place of
    /// its activity `replacing`. What Teams answers.
    pub(super) async fn write_activity(
        &self,
        app: &TeamsApplication,
        (service_url, conversation): (&str, &str),
        replacing: Option<&str>,
        body: &Value,
    ) -> Result<Value, TeamsFailure> {
        let mut segments = vec!["v3", "conversations", conversation, "activities"];
        segments.extend(replacing);
        let url = self.service(service_url, &segments)?;
        let token = self.token(app).await?;
        let client = &self.inner.client;
        let request = match replacing {
            None => client.post(url),
            Some(_) => client.put(url),
        };
        let request = request
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(body.to_string());
        self.answered(request.header(AUTHORIZATION, format!("Bearer {token}")))
            .await
    }

    /// Who the member `member` of the conversation `conversation` of the
    /// service `service_url` is, as Teams says it to the bot `app`.
    pub async fn member(
        &self,
        app: &TeamsApplication,
        (service_url, conversation): (&str, &str),
        member: &str,
    ) -> Result<TeamsMemberProfile, TeamsFailure> {
        let url = self.service(
            service_url,
            &["v3", "conversations", conversation, "members", member],
        )?;
        let token = self.token(app).await?;
        let request = (self.inner.client.get(url)).header(AUTHORIZATION, format!("Bearer {token}"));
        let answer = self.answered(request).await?;
        let field = |name: &str| {
            (answer.get(name).and_then(Value::as_str))
                .map(str::to_owned)
                .filter(|value| !value.is_empty())
        };
        Ok(TeamsMemberProfile {
            aad_object_id: field("aadObjectId"),
            tenant_id: field("tenantId"),
            email: field("email"),
            user_principal_name: field("userPrincipalName"),
            name: field("name"),
        })
    }

    /// A token for `app`, kept since it was given with the same secret, or a
    /// new one.
    async fn token(&self, app: &TeamsApplication) -> Result<String, TeamsFailure> {
        let secret = digest(&app.client_secret);
        if let Ok(tokens) = self.inner.tokens.lock()
            && let Some(kept) = tokens.get(&app.app_id)
            && kept.secret == secret
            && Instant::now() < kept.usable_until
        {
            return Ok(kept.token.clone());
        }
        self.new_token(app).await
    }

    /// A new token for `app`, from Microsoft Entra (client credentials),
    /// kept for later.
    async fn new_token(&self, app: &TeamsApplication) -> Result<String, TeamsFailure> {
        let mut url = self.inner.login.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty()
                .extend([app.directory_id.as_str(), "oauth2", "v2.0", "token"]);
        }
        let form = Serializer::new(String::new())
            .append_pair("grant_type", "client_credentials")
            .append_pair("client_id", &app.app_id)
            .append_pair("client_secret", &app.client_secret)
            .append_pair("scope", BOT_FRAMEWORK_SCOPE)
            .finish();
        let request = (self.inner.client.post(url))
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form);
        let answer = self.answered(request).await?;
        let token = (answer.get("access_token").and_then(Value::as_str))
            .filter(|token| !token.is_empty())
            .ok_or(TeamsFailure::Unreadable)?
            .to_owned();
        let lasts = answer
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let usable_until =
            Instant::now() + Duration::from_secs(lasts).saturating_sub(EXPIRY_MARGIN);
        if let Ok(mut tokens) = self.inner.tokens.lock() {
            let kept = CachedToken {
                secret: digest(&app.client_secret),
                token: token.clone(),
                usable_until,
            };
            tokens.insert(app.app_id.clone(), kept);
        }
        Ok(token)
    }

    /// The address `segments` under the service `service_url`, if it is a
    /// service the bot may write to.
    fn service(&self, service_url: &str, segments: &[&str]) -> Result<Url, TeamsFailure> {
        if !self.inner.services.trust(service_url) {
            return Err(TeamsFailure::Untrusted);
        }
        let mut url = Url::parse(service_url).map_err(|_| TeamsFailure::Untrusted)?;
        url.path_segments_mut()
            .map_err(|_| TeamsFailure::Untrusted)?
            .pop_if_empty()
            .extend(segments);
        Ok(url)
    }

    pub(super) fn openid(&self) -> &Url {
        &self.inner.openid
    }

    pub(super) fn cached_keys(&self) -> Option<KeyCache> {
        self.inner.keys.lock().ok().and_then(|keys| keys.clone())
    }

    pub(super) fn reading(&self) -> &tokio::sync::Mutex<()> {
        &self.inner.reading
    }

    pub(super) fn keep_keys(&self, cache: KeyCache) {
        if let Ok(mut keys) = self.inner.keys.lock() {
            *keys = Some(cache);
        }
    }

    /// The JSON at `url`.
    pub(super) async fn get_json(&self, url: Url) -> Result<Value, TeamsFailure> {
        self.answered(self.inner.client.get(url)).await
    }

    /// The JSON `request` is answered with within the delay, or why
    /// Microsoft refused.
    async fn answered(&self, request: RequestBuilder) -> Result<Value, TeamsFailure> {
        let request = request.header(ACCEPT, "application/json");
        let sent = tokio::time::timeout(TIMEOUT, async {
            let response = request.send().await?;
            let status = response.status().as_u16();
            let retry_after = (response.headers().get("retry-after"))
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            let body = read_bounded(response).await?;
            Ok::<_, reqwest_mcp::Error>((status, retry_after, body))
        })
        .await;
        let (status, retry_after, body) = match sent {
            Err(_) => return Err(TeamsFailure::Unanswered(TOO_SLOW)),
            Ok(Err(_)) => return Err(TeamsFailure::Unanswered(UNREACHABLE)),
            Ok(Ok(answer)) => answer,
        };
        let answer: Option<Value> = body.and_then(|body| serde_json::from_slice(&body).ok());
        if (200..300).contains(&status) {
            // A message written: Teams may answer with nothing to read.
            return Ok(answer.unwrap_or(Value::Null));
        }
        if status == 429 {
            return Err(TeamsFailure::Throttled { retry_after });
        }
        if status >= 500 {
            return Err(TeamsFailure::Failed(status));
        }
        let code = answer
            .as_ref()
            .and_then(|answer| match answer.get("error") {
                Some(Value::String(code)) => Some(code.clone()),
                Some(error) => error.get("code").and_then(Value::as_str).map(str::to_owned),
                None => None,
            });
        Err(TeamsFailure::Refused {
            status,
            cause: refusal_cause(status, code.as_deref().unwrap_or_default()),
        })
    }
}

/// A secret's digest: which secret a token was given for, never the
/// secret.
fn digest(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.as_bytes()).into()
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
