//! Telegram's Bot API as a company's gateway reaches it (ADR-133), with its
//! bot's token: one request at a time, bounded in time and in size, never
//! following a redirection (the hardened client), never repeated. The
//! token is part of each method's address (`/bot<token>/<method>`): no
//! address nor client error is ever printed, only what failed, in French.
//! Telegram answers `"ok": false`, with a status and a description, when
//! it refuses. Besides its messages, the bot shows buttons under them,
//! answers their press and takes them off (ADR-136).

use std::time::Duration;

use reqwest_mcp::Client;
use reqwest_mcp::header::{ACCEPT, CONTENT_TYPE};
use serde_json::{Value, json};
use url::Url;

use super::{TelegramBot, TelegramFailure, refusal_cause};
use crate::outbound::{TOO_SLOW, UNREACHABLE};
use mimorya_core::gateways::telegram::{ALLOWED_UPDATES, is_bot_username};

/// Telegram's Bot API.
pub const TELEGRAM_API: &str = "https://api.telegram.org";
/// The most of an answer read: a message sent comes back in a few
/// kibibytes.
const MAX_ANSWER_BYTES: usize = 256 * 1024;
/// How long Telegram has to answer.
const TIMEOUT: Duration = Duration::from_secs(10);
/// The most connections Telegram opens at once to send the updates.
const MAX_CONNECTIONS: u8 = 10;

/// Telegram's Bot API at `base`, with the hardened client.
#[derive(Clone)]
pub struct TelegramApi {
    base: Url,
    client: Client,
}

impl TelegramApi {
    pub fn new(base: &str, client: Client) -> anyhow::Result<Self> {
        Ok(Self {
            base: Url::parse(base)?,
            client,
        })
    }

    /// The bot the token is (`getMe`).
    pub async fn bot(&self, token: &str) -> Result<TelegramBot, TelegramFailure> {
        let result = self.call(token, "getMe", &json!({})).await?;
        let id = result.get("id").and_then(Value::as_i64);
        let username = result.get("username").and_then(Value::as_str);
        let is_bot = result.get("is_bot").and_then(Value::as_bool) == Some(true);
        match (id, username) {
            (Some(id), Some(username)) if is_bot && is_bot_username(username) && id > 0 => {
                Ok(TelegramBot {
                    id,
                    username: username.to_owned(),
                })
            }
            _ => Err(TelegramFailure::Unreadable),
        }
    }

    /// Tells Telegram to send the bot's updates to `url`, with `secret` in
    /// a header, a message and a button pressed only, forgetting those it
    /// kept (`setWebhook`).
    pub async fn set_webhook(
        &self,
        token: &str,
        url: &str,
        secret: &str,
    ) -> Result<(), TelegramFailure> {
        let body = json!({
            "url": url,
            "secret_token": secret,
            "allowed_updates": ALLOWED_UPDATES,
            "max_connections": MAX_CONNECTIONS,
            "drop_pending_updates": true,
        });
        self.call(token, "setWebhook", &body).await.map(drop)
    }

    /// Tells Telegram to send the bot's updates nowhere (`deleteWebhook`).
    pub async fn delete_webhook(&self, token: &str) -> Result<(), TelegramFailure> {
        let body = json!({ "drop_pending_updates": true });
        self.call(token, "deleteWebhook", &body).await.map(drop)
    }

    /// Writes `text` in the private chat of the person `chat`, as plain
    /// text, no link previewed (`sendMessage`); the message's number.
    pub async fn send(&self, token: &str, chat: i64, text: &str) -> Result<i64, TelegramFailure> {
        let body = json!({
            "chat_id": chat,
            "text": text,
            "link_preview_options": { "is_disabled": true },
        });
        self.send_message(token, &body).await
    }

    /// Writes `html` in the private chat of the person `chat`, in the HTML
    /// Telegram reads (ADR-135), no link previewed; the message's number.
    /// Telegram refuses a text it cannot read (`RefusalCause::Markup`).
    pub async fn send_html(
        &self,
        token: &str,
        chat: i64,
        html: &str,
    ) -> Result<i64, TelegramFailure> {
        let body = json!({
            "chat_id": chat,
            "text": html,
            "parse_mode": "HTML",
            "link_preview_options": { "is_disabled": true },
        });
        self.send_message(token, &body).await
    }

    /// Writes `text` (in Telegram's HTML when `html`) in the private chat
    /// of the person `chat`, with `keyboard`'s buttons under it (ADR-136);
    /// the message's number.
    pub async fn send_buttons(
        &self,
        token: &str,
        chat: i64,
        (text, html): (&str, bool),
        keyboard: &Value,
    ) -> Result<i64, TelegramFailure> {
        let mut body = json!({
            "chat_id": chat,
            "text": text,
            "link_preview_options": { "is_disabled": true },
            "reply_markup": keyboard,
        });
        if html {
            body["parse_mode"] = json!("HTML");
        }
        self.send_message(token, &body).await
    }

    /// Writes `text`, as plain text, in the private chat of the person
    /// `chat`, as an answer to its message `to` (ADR-136).
    pub async fn reply(
        &self,
        token: &str,
        chat: i64,
        to: i64,
        text: &str,
    ) -> Result<i64, TelegramFailure> {
        let body = json!({
            "chat_id": chat,
            "text": text,
            "link_preview_options": { "is_disabled": true },
            "reply_parameters": { "message_id": to, "allow_sending_without_reply": true },
        });
        self.send_message(token, &body).await
    }

    /// Answers the press `callback_id` of one of the bot's buttons with
    /// `text`, shown briefly to whoever pressed it (`answerCallbackQuery`).
    pub async fn answer_press(
        &self,
        token: &str,
        callback_id: &str,
        text: &str,
    ) -> Result<(), TelegramFailure> {
        let body = json!({ "callback_query_id": callback_id, "text": text });
        self.call(token, "answerCallbackQuery", &body)
            .await
            .map(drop)
    }

    /// Puts `keyboard`'s buttons under the message `message_id` of the
    /// private chat `chat`, in place of its own (`editMessageReplyMarkup`).
    pub async fn edit_buttons(
        &self,
        token: &str,
        chat: i64,
        message_id: i64,
        keyboard: &Value,
    ) -> Result<(), TelegramFailure> {
        let body = json!({ "chat_id": chat, "message_id": message_id, "reply_markup": keyboard });
        self.call(token, "editMessageReplyMarkup", &body)
            .await
            .map(drop)
    }

    /// Writes again the message `message_id` of the private chat `chat`:
    /// `text` (in Telegram's HTML when `html`), with `keyboard`'s buttons
    /// under it in place of its own (`editMessageText`, ADR-148).
    pub async fn edit_message(
        &self,
        token: &str,
        (chat, message_id): (i64, i64),
        (text, html): (&str, bool),
        keyboard: &Value,
    ) -> Result<(), TelegramFailure> {
        let mut body = json!({
            "chat_id": chat,
            "message_id": message_id,
            "text": text,
            "link_preview_options": { "is_disabled": true },
            "reply_markup": keyboard,
        });
        if html {
            body["parse_mode"] = json!("HTML");
        }
        self.call(token, "editMessageText", &body).await.map(drop)
    }

    async fn send_message(&self, token: &str, body: &Value) -> Result<i64, TelegramFailure> {
        let result = self.call(token, "sendMessage", body).await?;
        (result.get("message_id").and_then(Value::as_i64)).ok_or(TelegramFailure::Unreadable)
    }

    /// `method`'s address, with the token: never printed.
    fn method(&self, token: &str, method: &str) -> Url {
        let mut url = self.base.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty()
                .push(&format!("bot{token}"))
                .push(method);
        }
        url
    }

    /// The method called with `body`, its answer read within the delay:
    /// its result once `ok`, or why Telegram refused.
    async fn call(
        &self,
        token: &str,
        method: &str,
        body: &Value,
    ) -> Result<Value, TelegramFailure> {
        let body = serde_json::to_vec(body).map_err(|_| TelegramFailure::Unreadable)?;
        let request = self
            .client
            .post(self.method(token, method))
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .header(ACCEPT, "application/json")
            .body(body);
        let sent = tokio::time::timeout(TIMEOUT, async {
            let response = request.send().await?;
            let status = response.status().as_u16();
            let body = read_bounded(response).await?;
            Ok::<_, reqwest_mcp::Error>((status, body))
        })
        .await;
        let (status, body) = match sent {
            Err(_) => return Err(TelegramFailure::Unanswered(TOO_SLOW)),
            Ok(Err(_)) => return Err(TelegramFailure::Unanswered(UNREACHABLE)),
            Ok(Ok(answer)) => answer,
        };
        let answer: Option<Value> = body.and_then(|body| serde_json::from_slice(&body).ok());
        let Some(answer) = answer else {
            return Err(if (200..300).contains(&status) {
                TelegramFailure::Unreadable
            } else {
                TelegramFailure::Failed(status)
            });
        };
        if answer.get("ok").and_then(Value::as_bool) == Some(true) {
            return answer
                .get("result")
                .cloned()
                .ok_or(TelegramFailure::Unreadable);
        }
        refused(status, &answer)
    }
}

/// Why Telegram refused, as its `answer` says.
fn refused(status: u16, answer: &Value) -> Result<Value, TelegramFailure> {
    let code = (answer.get("error_code").and_then(Value::as_u64))
        .and_then(|code| u16::try_from(code).ok())
        .unwrap_or(status);
    if code == 429 {
        let retry_after = (answer.get("parameters"))
            .and_then(|parameters| parameters.get("retry_after"))
            .and_then(Value::as_u64)
            .map(Duration::from_secs);
        return Err(TelegramFailure::Throttled { retry_after });
    }
    let description = (answer.get("description").and_then(Value::as_str)).unwrap_or_default();
    Err(TelegramFailure::Refused {
        status: code,
        cause: refusal_cause(code, description),
    })
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
