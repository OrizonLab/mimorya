//! The methods of Telegram's Bot API the test Telegram answers, as Telegram
//! does: the bot a token is, its webhook, the messages it writes (in plain
//! text or HTML, with buttons under them), the answer to a button pressed
//! and the buttons taken off a message (ADR-133, ADR-135, ADR-136), or the
//! message written again (ADR-148).

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::html::readable_html;
use super::{
    OTHER_BOT, OTHER_BOT_TOKEN, Shared, TELEGRAM_BOT, TELEGRAM_BOT_USERNAME, TELEGRAM_TOKEN,
    TelegramMessage,
};

fn refusal(status: StatusCode, description: &str) -> Response {
    let body = json!({ "ok": false, "error_code": status.as_u16(), "description": description });
    (status, Json(body)).into_response()
}

fn answered(result: Value) -> Response {
    Json(json!({ "ok": true, "result": result })).into_response()
}

pub(super) async fn call(
    State(shared): State<Arc<Mutex<Shared>>>,
    Path((bot, method)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    let mut shared = shared.lock().expect("lock");
    if shared.down {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let (id, username) = match bot.strip_prefix("bot") {
        Some(TELEGRAM_TOKEN) if !shared.revoked => (TELEGRAM_BOT, TELEGRAM_BOT_USERNAME),
        Some(OTHER_BOT_TOKEN) if !shared.revoked => (OTHER_BOT, "AutreEntrepriseBot"),
        _ => return refusal(StatusCode::UNAUTHORIZED, "Unauthorized"),
    };
    match method.as_str() {
        "getMe" => answered(json!({
            "id": id, "is_bot": true, "first_name": "Assistant", "username": username,
            "can_join_groups": false, "can_read_all_group_messages": false
        })),
        "setWebhook" if shared.webhooks_refused => refusal(
            StatusCode::BAD_REQUEST,
            "Bad Request: bad webhook: HTTPS url must be provided for webhook",
        ),
        "setWebhook" => {
            let field = |name: &str| body.get(name).and_then(Value::as_str).map(str::to_owned);
            let (Some(url), Some(secret)) = (field("url"), field("secret_token")) else {
                return refusal(StatusCode::BAD_REQUEST, "Bad Request: url is empty");
            };
            shared.webhooks.insert(id, (url, secret));
            answered(json!(true))
        }
        "deleteWebhook" => {
            shared.webhooks.remove(&id);
            answered(json!(true))
        }
        "sendMessage" => {
            let chat = body.get("chat_id").and_then(Value::as_i64);
            let text = body.get("text").and_then(Value::as_str);
            let (Some(chat), Some(text)) = (chat, text) else {
                return refusal(
                    StatusCode::BAD_REQUEST,
                    "Bad Request: message text is empty",
                );
            };
            if shared.blocked.contains(&chat) {
                return refusal(
                    StatusCode::FORBIDDEN,
                    "Forbidden: bot was blocked by the user",
                );
            }
            let html = body.get("parse_mode").and_then(Value::as_str) == Some("HTML");
            if html && (shared.html_refused || !readable_html(text)) {
                return refusal(
                    StatusCode::BAD_REQUEST,
                    "Bad Request: can't parse entities: Unsupported start tag",
                );
            }
            let reply_to = (body.get("reply_parameters"))
                .and_then(|r| r.get("message_id"))
                .and_then(Value::as_i64);
            shared.messages.push(TelegramMessage {
                bot: id,
                chat,
                text: text.to_owned(),
                html,
                keyboard: body.get("reply_markup").cloned(),
                reply_to,
                edits: 0,
            });
            answered(json!({ "message_id": shared.messages.len(), "chat": { "id": chat } }))
        }
        "answerCallbackQuery" => {
            let field = |name: &str| body.get(name).and_then(Value::as_str).map(str::to_owned);
            let Some(callback) = field("callback_query_id") else {
                return refusal(StatusCode::BAD_REQUEST, "Bad Request: query is too old");
            };
            shared
                .answers
                .push((callback, field("text").unwrap_or_default()));
            answered(json!(true))
        }
        "editMessageReplyMarkup" | "editMessageText" => {
            let chat = body.get("chat_id").and_then(Value::as_i64);
            let number = body.get("message_id").and_then(Value::as_i64);
            let index = number.and_then(|n| usize::try_from(n - 1).ok());
            let html = body.get("parse_mode").and_then(Value::as_str) == Some("HTML");
            let text = body.get("text").and_then(Value::as_str);
            if method == "editMessageText" {
                let readable =
                    text.is_some_and(|t| !html || (!shared.html_refused && readable_html(t)));
                if !readable {
                    return refusal(
                        StatusCode::BAD_REQUEST,
                        "Bad Request: can't parse entities: Unsupported start tag",
                    );
                }
            }
            let edited = index
                .and_then(|i| shared.messages.get_mut(i))
                .filter(|m| m.bot == id && Some(m.chat) == chat);
            let Some(message) = edited else {
                return refusal(
                    StatusCode::BAD_REQUEST,
                    "Bad Request: message to edit not found",
                );
            };
            let keyboard = body.get("reply_markup").cloned();
            if let Some(text) = text.filter(|_| method == "editMessageText") {
                if message.text == text && message.html == html && message.keyboard == keyboard {
                    return refusal(
                        StatusCode::BAD_REQUEST,
                        "Bad Request: message is not modified: specified new message content \
                         and reply markup are exactly the same as a current content and reply \
                         markup of the message",
                    );
                }
                message.text = text.to_owned();
                message.html = html;
                message.edits += 1;
            }
            message.keyboard = keyboard;
            answered(json!(true))
        }
        _ => refusal(StatusCode::NOT_FOUND, "Not Found"),
    }
}
