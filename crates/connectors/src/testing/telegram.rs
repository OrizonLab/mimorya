//! Telegram's Bot API on a local port, for tests (feature `testing`,
//! ADR-133). Its bot's token, `TELEGRAM_TOKEN`, is the bot `TELEGRAM_BOT`
//! (`@NeoMaintenanceBot`); `OTHER_BOT_TOKEN` is another bot's; any other
//! token is refused (401), as one a test revoked. It keeps the webhook each
//! bot was given, with its secret, and the messages the bots write; a
//! message in HTML (ADR-135) is refused, as Telegram does, when a tag is
//! not one it reads or not closed where it opens, or an `&` starts no
//! entity. A test may make it unavailable (503), refuse webhooks (as one
//! not in HTTPS), refuse any HTML, or have a person block the bot. For the end-to-end tests (ADR-134), the
//! messages are also listed at `GET /test/messages`, and each bot's webhook
//! with its secret at `GET /test/webhooks`: whoever reaches the test
//! Telegram can send updates as Telegram does, which only a test server
//! offers. `update` writes an update as Telegram sends it.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

/// The bot's token, as BotFather gives them, and the bot it is.
pub const TELEGRAM_TOKEN: &str = "123456789:AAH-fictif_jeton_du_bot_neo_maint_0001";
pub const TELEGRAM_BOT: i64 = 123_456_789;
pub const TELEGRAM_BOT_USERNAME: &str = "NeoMaintenanceBot";
/// Another bot's token, and the bot it is.
pub const OTHER_BOT_TOKEN: &str = "987654321:AAH-fictif_jeton_dun_autre_bot_000001";
pub const OTHER_BOT: i64 = 987_654_321;

/// A message a bot wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramMessage {
    /// The bot that wrote it.
    pub bot: i64,
    /// The private chat it was written in: the person's identifier.
    pub chat: i64,
    pub text: String,
    /// Whether it was written in HTML (`parse_mode`).
    pub html: bool,
}

#[derive(Default)]
struct Shared {
    revoked: bool,
    down: bool,
    webhooks_refused: bool,
    html_refused: bool,
    blocked: HashSet<i64>,
    /// Each bot's webhook: its address and secret.
    webhooks: HashMap<i64, (String, String)>,
    messages: Vec<TelegramMessage>,
}

pub struct FakeTelegram {
    /// Its address, `http://127.0.0.1:<port>`, as the gateways are given it.
    pub url: String,
    shared: Arc<Mutex<Shared>>,
}

impl FakeTelegram {
    pub async fn start() -> Self {
        Self::start_on("127.0.0.1:0").await
    }

    /// The same, at `address`: where the end-to-end tests read what the
    /// bots wrote and the webhook each was given (ADR-134).
    pub async fn start_on(address: &str) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let router = Router::new()
            .route("/{bot}/{method}", post(call))
            .route("/test/messages", get(messages))
            .route("/test/webhooks", get(webhooks))
            .with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .expect("local port");
        let address: SocketAddr = listener.local_addr().expect("local address");
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("test Telegram");
        });
        Self {
            url: format!("http://{address}"),
            shared,
        }
    }

    /// Every token is refused from now on, as one revoked.
    pub fn revoke(&self) {
        self.shared.lock().expect("lock").revoked = true;
    }

    /// Every method answers 503 while `down`.
    pub fn unavailable(&self, down: bool) {
        self.shared.lock().expect("lock").down = down;
    }

    /// Webhooks are refused while `refused`, as an address not in HTTPS.
    pub fn refuse_webhooks(&self, refused: bool) {
        self.shared.lock().expect("lock").webhooks_refused = refused;
    }

    /// Every message in HTML is refused while `refused`, as one Telegram
    /// cannot read.
    pub fn refuse_html(&self, refused: bool) {
        self.shared.lock().expect("lock").html_refused = refused;
    }

    /// The person `chat` blocked the bots.
    pub fn block(&self, chat: i64) {
        self.shared.lock().expect("lock").blocked.insert(chat);
    }

    /// The webhook the bot `bot` was given, with its secret, if any.
    pub fn webhook(&self, bot: i64) -> Option<(String, String)> {
        self.shared
            .lock()
            .expect("lock")
            .webhooks
            .get(&bot)
            .cloned()
    }

    /// The messages the bots wrote, oldest first.
    pub fn messages(&self) -> Vec<TelegramMessage> {
        self.shared.lock().expect("lock").messages.clone()
    }
}

/// An update as Telegram sends it: the person `from` (named `username`)
/// writing `text` in their private chat with the bot.
pub fn update(update_id: i64, from: i64, username: &str, text: &str) -> String {
    json!({
        "update_id": update_id,
        "message": {
            "message_id": update_id,
            "from": { "id": from, "is_bot": false, "first_name": "Test", "username": username },
            "chat": { "id": from, "type": "private", "first_name": "Test" },
            "date": 1_700_000_000,
            "text": text
        }
    })
    .to_string()
}

fn refusal(status: StatusCode, description: &str) -> Response {
    let body = json!({ "ok": false, "error_code": status.as_u16(), "description": description });
    (status, Json(body)).into_response()
}

fn answered(result: Value) -> Response {
    Json(json!({ "ok": true, "result": result })).into_response()
}

async fn call(
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
            shared.messages.push(TelegramMessage {
                bot: id,
                chat,
                text: text.to_owned(),
                html,
            });
            answered(json!({ "message_id": shared.messages.len(), "chat": { "id": chat } }))
        }
        _ => refusal(StatusCode::NOT_FOUND, "Not Found"),
    }
}

/// Whether Telegram reads `text` as HTML: only the tags it knows (`<a>`
/// with its `href` alone), each closed where it opens, and `&` starting
/// one of its entities.
fn readable_html(text: &str) -> bool {
    const TAGS: [&str; 5] = ["b", "i", "code", "pre", "a"];
    const ENTITIES: [&str; 4] = ["&amp;", "&lt;", "&gt;", "&quot;"];
    let mut open: Vec<&str> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(['<', '&', '>']) {
        let tail = &rest[at..];
        if tail.starts_with('>') {
            return false;
        }
        if tail.starts_with('&') {
            let Some(entity) = ENTITIES.iter().find(|e| tail.starts_with(**e)) else {
                return false;
            };
            rest = &tail[entity.len()..];
            continue;
        }
        let Some(end) = tail.find('>') else {
            return false;
        };
        let tag = &tail[1..end];
        if let Some(name) = tag.strip_prefix('/') {
            if open.pop() != Some(name) {
                return false;
            }
        } else {
            let (name, attributes) = tag.split_once(' ').unwrap_or((tag, ""));
            let href = attributes
                .strip_prefix("href=\"")
                .and_then(|a| a.strip_suffix('"'));
            let attributes_read = match name {
                "a" => href.is_some_and(|h| !h.contains('"')),
                _ => attributes.is_empty(),
            };
            if !TAGS.contains(&name) || !attributes_read {
                return false;
            }
            open.push(name);
        }
        rest = &tail[end + 1..];
    }
    open.is_empty()
}

/// The messages the bots wrote, oldest first, for the end-to-end tests.
async fn messages(State(shared): State<Arc<Mutex<Shared>>>) -> Json<Value> {
    let shared = shared.lock().expect("lock");
    let messages: Vec<Value> = (shared.messages.iter())
        .map(|m| json!({ "bot": m.bot, "chat": m.chat, "text": m.text, "html": m.html }))
        .collect();
    Json(json!(messages))
}

/// The webhook each bot was given, with its secret, for the end-to-end
/// tests to send updates as Telegram does.
async fn webhooks(State(shared): State<Arc<Mutex<Shared>>>) -> Json<Value> {
    let shared = shared.lock().expect("lock");
    let webhooks: Vec<Value> = (shared.webhooks.iter())
        .map(|(bot, (url, secret))| json!({ "bot": bot, "url": url, "secret": secret }))
        .collect();
    Json(json!(webhooks))
}

#[cfg(test)]
mod tests {
    use super::readable_html;

    #[test]
    fn only_html_telegram_reads_is_taken() {
        assert!(readable_html(
            "<b>a</b> <i>b <b>c</b></i> <code>&lt;d&gt;</code> <pre>e &amp; f</pre> \
             <a href=\"https://x.example/?a=1&amp;b=&quot;2&quot;\">g</a>"
        ));
        for text in [
            "<b>ouvert",
            "<b>a</i>",
            "</b>",
            "<u>souligné</u>",
            "<b class=\"x\">a</b>",
            "<a>lien</a>",
            "a & b",
            "a > b",
            "<b",
        ] {
            assert!(!readable_html(text), "{text}");
        }
    }
}
