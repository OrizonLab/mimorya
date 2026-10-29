//! A button the bot showed, pressed by a person (ADR-136): Telegram sends
//! it as a `callback_query`, with the message the button is under as
//! Telegram holds it. Only a press in the person's own private chat with
//! the bot is read, with who sent that message and what each of its
//! buttons carries: the button pressed is then known to be one the bot
//! showed there, whatever a client claims it pressed.

use serde::Deserialize;

use super::update::{RawChat, RawUser, TelegramUser, is_user_id, shown_name};

/// The most a button carries, as Telegram allows.
const MAX_DATA_BYTES: usize = 64;
/// The most read of a callback query's identifier.
const MAX_CALLBACK_ID_CHARS: usize = 64;
/// The most rows, and buttons in a row, a message's keyboard is read with.
const MAX_ROWS: usize = 20;
const MAX_ROW_BUTTONS: usize = 8;
/// The most kept of what a button says.
const MAX_LABEL_CHARS: usize = 64;

#[derive(Deserialize)]
pub(super) struct RawCallback {
    id: String,
    from: RawUser,
    #[serde(default)]
    message: Option<RawPressedMessage>,
    #[serde(default)]
    data: Option<String>,
}

#[derive(Deserialize)]
struct RawPressedMessage {
    message_id: i64,
    #[serde(default)]
    from: Option<RawUser>,
    chat: RawChat,
    #[serde(default)]
    reply_markup: Option<RawKeyboard>,
}

#[derive(Deserialize)]
struct RawKeyboard {
    #[serde(default)]
    inline_keyboard: Vec<Vec<RawButton>>,
}

#[derive(Deserialize)]
struct RawButton {
    #[serde(default)]
    text: String,
    #[serde(default)]
    callback_data: Option<String>,
}

/// A press of one of the bot's buttons, in the presser's private chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonPress {
    /// The callback query, answered once taken.
    pub callback_id: String,
    pub person: TelegramUser,
    /// The message the button is under: its number in the chat.
    pub message_id: i64,
    /// Who sent that message, by their identifier: the bot, for its own.
    pub sent_by: Option<i64>,
    /// What the button pressed carries.
    pub data: String,
    /// The message's buttons, row by row, as Telegram holds them.
    pub keyboard: Vec<Vec<KeyboardButton>>,
}

/// A button of a message: what it says and what it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardButton {
    pub text: String,
    pub data: String,
}

impl ButtonPress {
    /// Whether the button pressed is one of its message's.
    pub fn shown(&self) -> bool {
        (self.keyboard.iter().flatten()).any(|button| button.data == self.data)
    }
}

/// The press, if a person pressed one of a message's buttons in their own
/// private chat with the bot.
pub(super) fn button_press(raw: RawCallback) -> Option<ButtonPress> {
    let message = raw.message?;
    let from = raw.from;
    let private = message.chat.kind == "private" && message.chat.id == from.id;
    if !private || from.is_bot || !is_user_id(from.id) {
        return None;
    }
    let data = raw
        .data
        .filter(|d| !d.is_empty() && d.len() <= MAX_DATA_BYTES)?;
    let callback_named = !raw.id.is_empty() && raw.id.chars().count() <= MAX_CALLBACK_ID_CHARS;
    let rows = message
        .reply_markup
        .map(|m| m.inline_keyboard)
        .unwrap_or_default();
    if !callback_named || rows.len() > MAX_ROWS || rows.iter().any(|r| r.len() > MAX_ROW_BUTTONS) {
        return None;
    }
    let keyboard = (rows.into_iter())
        .map(|row| {
            (row.into_iter())
                .filter_map(|button| {
                    Some(KeyboardButton {
                        text: button.text.chars().take(MAX_LABEL_CHARS).collect(),
                        data: button.callback_data?,
                    })
                })
                .collect()
        })
        .collect();
    Some(ButtonPress {
        callback_id: raw.id,
        person: TelegramUser {
            id: from.id,
            name: shown_name(&from),
        },
        message_id: message.message_id,
        sent_by: message.from.map(|sender| sender.id),
        data,
        keyboard,
    })
}

#[cfg(test)]
mod tests {
    use super::super::update::{Update, read_update};
    use super::*;
    use serde_json::{Value, json};

    fn pressed(change: impl FnOnce(&mut Value)) -> Update {
        let mut update = json!({
            "update_id": 813,
            "callback_query": {
                "id": "4382bfdwdsb323b2d9",
                "from": { "id": 4242, "is_bot": false, "first_name": "Léa", "username": "lea_font" },
                "message": {
                    "message_id": 9,
                    "from": { "id": 123_456_789, "is_bot": true, "first_name": "Bot" },
                    "chat": { "id": 4242, "type": "private" },
                    "date": 1_700_000_000,
                    "text": "Une action attend",
                    "reply_markup": { "inline_keyboard": [[
                        { "text": "Valider", "callback_data": "v:1" },
                        { "text": "Refuser", "callback_data": "r:1" }
                    ]] }
                },
                "chat_instance": "-1",
                "data": "v:1"
            }
        });
        change(&mut update);
        read_update(update.to_string().as_bytes()).unwrap().content
    }

    #[test]
    fn a_press_of_a_button_in_a_person_s_private_chat_is_read() {
        let Update::Press(press) = pressed(|_| {}) else {
            panic!("a press");
        };
        assert_eq!(press.callback_id, "4382bfdwdsb323b2d9");
        assert_eq!(press.person.id, 4242);
        assert_eq!(press.message_id, 9);
        assert_eq!(press.sent_by, Some(123_456_789));
        assert_eq!(press.data, "v:1");
        assert_eq!(press.keyboard[0][1].data, "r:1");
        assert!(press.shown());
        // A button its message does not carry was not shown there.
        let Update::Press(forged) = pressed(|u| u["callback_query"]["data"] = json!("v:2")) else {
            panic!("a press");
        };
        assert!(!forged.shown());
    }

    #[test]
    fn no_other_press_is() {
        for change in [
            |u: &mut Value| u["callback_query"]["message"]["chat"]["id"] = json!(5151),
            |u: &mut Value| u["callback_query"]["message"]["chat"]["type"] = json!("group"),
            |u: &mut Value| u["callback_query"]["from"]["is_bot"] = json!(true),
            |u: &mut Value| u["callback_query"]["data"] = json!("v".repeat(65)),
            |u: &mut Value| u["callback_query"]["data"] = json!(""),
            |u: &mut Value| u["callback_query"]["id"] = json!(""),
            |u: &mut Value| {
                u["callback_query"]
                    .as_object_mut()
                    .unwrap()
                    .remove("message");
            },
            |u: &mut Value| {
                u["callback_query"].as_object_mut().unwrap().remove("data");
            },
        ] {
            assert_eq!(pressed(change), Update::Other);
        }
    }

    #[test]
    fn a_keyboard_beyond_telegram_s_bounds_is_not_read() {
        let button = json!({ "text": "Valider", "callback_data": "v:1" });
        let too_many_rows = json!(vec![json!([button.clone()]); MAX_ROWS + 1]);
        let too_wide_row = json!([vec![button.clone(); MAX_ROW_BUTTONS + 1]]);
        for keyboard in [too_many_rows, too_wide_row] {
            let update = pressed(|u| {
                u["callback_query"]["message"]["reply_markup"]["inline_keyboard"] = keyboard;
            });
            assert_eq!(update, Update::Other);
        }
        // At the bounds, read.
        let full = json!(vec![json!(vec![button; MAX_ROW_BUTTONS]); MAX_ROWS]);
        let update = pressed(|u| {
            u["callback_query"]["message"]["reply_markup"]["inline_keyboard"] = full;
        });
        assert!(matches!(update, Update::Press(_)));
    }

    #[test]
    fn a_message_without_its_sender_is_no_one_s() {
        let Update::Press(press) = pressed(|u| {
            u["callback_query"]["message"]
                .as_object_mut()
                .unwrap()
                .remove("from");
        }) else {
            panic!("a press");
        };
        assert_eq!(press.sent_by, None);
    }
}
