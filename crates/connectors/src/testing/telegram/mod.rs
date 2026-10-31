//! Telegram's Bot API on a local port, for tests (feature `testing`,
//! ADR-133). Its bot's token, `TELEGRAM_TOKEN`, is the bot `TELEGRAM_BOT`
//! (`@NeoMaintenanceBot`); `OTHER_BOT_TOKEN` is another bot's; any other
//! token is refused (401), as one a test revoked. It keeps the webhook each
//! bot was given, with its secret, and the messages the bots write; a
//! message in HTML (ADR-135) is refused, as Telegram does, when a tag is
//! not one it reads or not closed where it opens, or an `&` starts no
//! entity. It keeps the buttons under a message, the answers to their
//! press, and takes them off as asked (ADR-136); `press` writes, as Telegram
//! sends it, a person's press of a message's button. A test may make it
//! unavailable (503), refuse webhooks (as one not in HTTPS), refuse any
//! HTML, or have a person block the bot. For the end-to-end tests
//! (ADR-134), the messages are also listed at `GET /test/messages`, with
//! their number and their buttons, and each bot's webhook with its secret
//! at `GET /test/webhooks`: whoever reaches the test Telegram can send
//! updates as Telegram does, which only a test server offers. `update`
//! writes an update as Telegram sends it.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

mod html;
mod methods;

use methods::call;

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
    /// The buttons under it, as they stand (`reply_markup`), if any.
    pub keyboard: Option<Value>,
    /// The message of the chat it answers, if any.
    pub reply_to: Option<i64>,
    /// How many times its text was written again (`editMessageText`).
    pub edits: usize,
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
    /// What the bots wrote: a message's number is its place, from 1.
    messages: Vec<TelegramMessage>,
    /// What the bots answered each button pressed: its callback, the text.
    answers: Vec<(String, String)>,
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

    /// The messages the bots wrote, oldest first: the message numbered `n`
    /// is the `n`-th.
    pub fn messages(&self) -> Vec<TelegramMessage> {
        self.shared.lock().expect("lock").messages.clone()
    }

    /// What the bots answered the buttons pressed: the callback, the text.
    pub fn answers(&self) -> Vec<(String, String)> {
        self.shared.lock().expect("lock").answers.clone()
    }

    /// The person `from` (named `username`) pressing, under the message
    /// numbered `message_id`, the button carrying `data`, as Telegram sends
    /// it as its update `update_id`: with the message as it stands, its
    /// buttons included (whether or not one carries `data`).
    pub fn press(
        &self,
        update_id: i64,
        (from, username): (i64, &str),
        message_id: i64,
        data: &str,
    ) -> String {
        let shared = self.shared.lock().expect("lock");
        let index = usize::try_from(message_id - 1).expect("a message's number");
        let message = shared.messages.get(index).expect("the message");
        let mut pressed = json!({
            "message_id": message_id,
            "from": { "id": message.bot, "is_bot": true, "first_name": "Assistant" },
            "chat": { "id": message.chat, "type": "private", "first_name": "Test" },
            "date": 1_700_000_000,
            "text": message.text,
        });
        if let Some(keyboard) = &message.keyboard {
            pressed["reply_markup"] = keyboard.clone();
        }
        json!({
            "update_id": update_id,
            "callback_query": {
                "id": format!("rappel-{update_id}"),
                "from": { "id": from, "is_bot": false, "first_name": "Test", "username": username },
                "message": pressed,
                "chat_instance": "-1",
                "data": data,
            }
        })
        .to_string()
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

/// The messages the bots wrote, oldest first, for the end-to-end tests.
async fn messages(State(shared): State<Arc<Mutex<Shared>>>) -> Json<Value> {
    let shared = shared.lock().expect("lock");
    let messages: Vec<Value> = (shared.messages.iter().enumerate())
        .map(|(i, m)| {
            json!({
                "messageId": i + 1, "bot": m.bot, "chat": m.chat, "text": m.text,
                "html": m.html, "keyboard": m.keyboard, "replyTo": m.reply_to,
            })
        })
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
