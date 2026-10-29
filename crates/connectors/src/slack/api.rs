//! Slack's Web API as a company's gateway reaches it (ADR-125), with its
//! bot's token: one request at a time, bounded in time and in size, never
//! following a redirection (the hardened client), never repeated. Slack
//! answers HTTP 200 with `"ok": false` and an error code when it refuses.

use std::time::Duration;

use reqwest_mcp::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue, RETRY_AFTER};
use reqwest_mcp::{Client, RequestBuilder};
use serde_json::{Value, json};
use url::Url;

use super::{SlackConversation, SlackFailure, SlackPerson, SlackWorkspace};
use crate::outbound::{TOO_SLOW, UNREACHABLE};
use mimorya_core::gateways::slack::{granted_scopes, is_person, is_workspace};

/// Slack's Web API.
pub const SLACK_API: &str = "https://slack.com/api";
/// The most of an answer read: a person's profile is far less.
const MAX_ANSWER_BYTES: usize = 256 * 1024;
/// How long Slack has to answer.
const TIMEOUT: Duration = Duration::from_secs(10);
/// What is kept of a workspace's name.
const MAX_WORKSPACE_NAME_CHARS: usize = 200;
/// The header Slack says a token's permissions in.
const OAUTH_SCOPES: &str = "x-oauth-scopes";

/// Slack's Web API at `base`, with the hardened client.
#[derive(Clone)]
pub struct SlackApi {
    base: Url,
    client: Client,
}

impl SlackApi {
    pub fn new(base: &str, client: Client) -> anyhow::Result<Self> {
        let mut base = Url::parse(base)?;
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        Ok(Self { base, client })
    }

    /// The workspace and bot user the token is the application's in, and
    /// the permissions Slack says it has (`auth.test`).
    pub async fn workspace(&self, token: &str) -> Result<SlackWorkspace, SlackFailure> {
        let (answer, scopes) = self
            .exchange(token, self.client.post(self.method("auth.test")))
            .await?;
        let field = |name: &str| answer.get(name).and_then(Value::as_str);
        let (Some(workspace_id), Some(bot_user_id)) = (field("team_id"), field("user_id")) else {
            return Err(SlackFailure::Unreadable);
        };
        if !is_workspace(workspace_id) || !is_person(bot_user_id) {
            return Err(SlackFailure::Unreadable);
        }
        // A bot's token names its bot (`bot_id`); a person's does not.
        if field("bot_id").is_none() {
            return Err(SlackFailure::Refused("not_a_bot".into()));
        }
        let name: String = field("team")
            .unwrap_or(workspace_id)
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_WORKSPACE_NAME_CHARS)
            .collect();
        Ok(SlackWorkspace {
            workspace_id: workspace_id.to_owned(),
            workspace_name: if name.trim().is_empty() {
                workspace_id.to_owned()
            } else {
                name.trim().to_owned()
            },
            bot_user_id: bot_user_id.to_owned(),
            scopes: scopes.as_deref().and_then(granted_scopes),
        })
    }

    /// Who `person` is, as Slack knows them (`users.info`).
    pub async fn person(&self, token: &str, person: &str) -> Result<SlackPerson, SlackFailure> {
        let mut url = self.method("users.info");
        url.query_pairs_mut().append_pair("user", person);
        let answer = self.send(token, self.client.get(url)).await?;
        let user = answer.get("user").ok_or(SlackFailure::Unreadable)?;
        let field = |name: &str| user.get(name).and_then(Value::as_str);
        let flag = |name: &str| user.get(name).and_then(Value::as_bool).unwrap_or(false);
        if field("id") != Some(person) {
            return Err(SlackFailure::Unreadable);
        }
        let address = user
            .get("profile")
            .and_then(|profile| profile.get("email"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(SlackPerson {
            id: person.to_owned(),
            workspace_id: field("team_id").map(str::to_owned),
            address,
            deleted: flag("deleted"),
            bot: flag("is_bot") || flag("is_app_user"),
        })
    }

    /// The conversation `channel`, as Slack knows it (`conversations.info`,
    /// with `im:read` for a direct one).
    pub async fn conversation(
        &self,
        token: &str,
        channel: &str,
    ) -> Result<SlackConversation, SlackFailure> {
        let mut url = self.method("conversations.info");
        url.query_pairs_mut().append_pair("channel", channel);
        let answer = self.send(token, self.client.get(url)).await?;
        conversation_in(&answer, channel)
    }

    /// Posts `text` in the conversation `channel`, in the thread `thread_ts`
    /// if any (`chat.postMessage`); the message's timestamp. Slack reads
    /// `text` as its markup: whoever builds it escapes what it quotes.
    pub async fn post(
        &self,
        token: &str,
        channel: &str,
        thread_ts: Option<&str>,
        text: &str,
    ) -> Result<String, SlackFailure> {
        self.post_blocks(token, channel, thread_ts, text, None)
            .await
    }

    /// The same, shown as `blocks` if any (buttons among them, ADR-129),
    /// `text` then being what notifications show.
    pub async fn post_blocks(
        &self,
        token: &str,
        channel: &str,
        thread_ts: Option<&str>,
        text: &str,
        blocks: Option<&[Value]>,
    ) -> Result<String, SlackFailure> {
        let mut body = json!({ "channel": channel, "text": text, "unfurl_links": false });
        if let Some(thread_ts) = thread_ts {
            body["thread_ts"] = json!(thread_ts);
        }
        if let Some(blocks) = blocks {
            body["blocks"] = json!(blocks);
        }
        let answer = self.send_json(token, "chat.postMessage", &body).await?;
        answer
            .get("ts")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(SlackFailure::Unreadable)
    }

    /// Rewrites the application's message `ts` of `channel` as `text` shown
    /// as `blocks` (`chat.update`).
    pub async fn update(
        &self,
        token: &str,
        channel: &str,
        ts: &str,
        text: &str,
        blocks: &[Value],
    ) -> Result<(), SlackFailure> {
        let body = json!({ "channel": channel, "ts": ts, "text": text, "blocks": blocks });
        self.send_json(token, "chat.update", &body).await.map(drop)
    }

    async fn send_json(
        &self,
        token: &str,
        method: &str,
        body: &Value,
    ) -> Result<Value, SlackFailure> {
        let body = serde_json::to_vec(body).map_err(|_| SlackFailure::Unreadable)?;
        let request = self
            .client
            .post(self.method(method))
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(body);
        self.send(token, request).await
    }

    fn method(&self, method: &str) -> Url {
        let mut url = self.base.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().push(method);
        }
        url
    }

    /// The request sent with the bot's token, its answer read within the
    /// delay: Slack's answer once `ok`, or why it refused.
    async fn send(&self, token: &str, request: RequestBuilder) -> Result<Value, SlackFailure> {
        self.exchange(token, request)
            .await
            .map(|(answer, _)| answer)
    }

    /// The same, with the permissions Slack says the token has
    /// (`x-oauth-scopes`), if it says them.
    async fn exchange(
        &self,
        token: &str,
        request: RequestBuilder,
    ) -> Result<(Value, Option<String>), SlackFailure> {
        let mut bearer = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| SlackFailure::Refused("invalid_auth".into()))?;
        bearer.set_sensitive(true);
        let request = request
            .header(AUTHORIZATION, bearer)
            .header(ACCEPT, "application/json");
        let sent = tokio::time::timeout(TIMEOUT, async {
            let response = request.send().await?;
            let status = response.status().as_u16();
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let scopes = (response.headers().get(OAUTH_SCOPES))
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let body = read_bounded(response).await?;
            Ok::<_, reqwest_mcp::Error>((status, retry_after, scopes, body))
        })
        .await;
        let (status, retry_after, scopes, body) = match sent {
            Err(_) => return Err(SlackFailure::Unanswered(TOO_SLOW)),
            Ok(Err(_)) => return Err(SlackFailure::Unanswered(UNREACHABLE)),
            Ok(Ok(answer)) => answer,
        };
        if status == 429 {
            return Err(SlackFailure::Throttled {
                retry_after: retry_after.map(Duration::from_secs),
            });
        }
        if !(200..300).contains(&status) {
            return Err(SlackFailure::Failed(status));
        }
        let answer: Value = body
            .and_then(|body| serde_json::from_slice(&body).ok())
            .ok_or(SlackFailure::Unreadable)?;
        if answer.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok((answer, scopes));
        }
        let code = answer
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown_error");
        Err(SlackFailure::Refused(code.to_owned()))
    }
}

/// The conversation `channel` as Slack's `answer` describes it: none other
/// is read; direct only if Slack says so; whom, only a person.
pub(super) fn conversation_in(
    answer: &Value,
    channel: &str,
) -> Result<SlackConversation, SlackFailure> {
    let found = answer.get("channel").ok_or(SlackFailure::Unreadable)?;
    if found.get("id").and_then(Value::as_str) != Some(channel) {
        return Err(SlackFailure::Unreadable);
    }
    let direct = found.get("is_im").and_then(Value::as_bool) == Some(true);
    let person = (found.get("user").and_then(Value::as_str))
        .filter(|person| is_person(person))
        .map(str::to_owned);
    Ok(SlackConversation { direct, person })
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
