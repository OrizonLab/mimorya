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
