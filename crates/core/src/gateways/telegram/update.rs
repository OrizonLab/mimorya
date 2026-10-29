//! What Telegram sends the company's bot (ADR-133), once its secret is
//! checked: an update, named by its number (Telegram sends one again when
//! it was not answered in time, and a retry is taken once). Of the updates,
//! only a person's own text message to the bot, in their private chat with
//! it, reaches the platform: never a bot's, never one in a group, a channel
//! or anywhere else, never an edit (Telegram sends those apart); and their
//! press of one of the bot's buttons there (`press`, ADR-136).

use serde::Deserialize;

use super::press::{ButtonPress, RawCallback, button_press};
use crate::plain_text::is_hidden;

/// The most text taken of a message (Telegram allows 4 096 characters; a
/// person's turn in the application is bounded alike).
pub const MAX_MESSAGE_CHARS: usize = 16_000;
/// The most kept of a person's name.
const MAX_NAME_CHARS: usize = 64;
/// The highest user identifier Telegram gives (52 bits).
const MAX_USER_ID: i64 = (1 << 52) - 1;

#[derive(Deserialize)]
struct RawUpdate {
    update_id: i64,
    #[serde(default)]
    message: Option<RawMessage>,
    #[serde(default)]
    callback_query: Option<RawCallback>,
}

#[derive(Deserialize)]
struct RawMessage {
    message_id: i64,
    #[serde(default)]
    from: Option<RawUser>,
    chat: RawChat,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct RawUser {
    pub(super) id: i64,
    #[serde(default)]
    pub(super) is_bot: bool,
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: Option<String>,
    #[serde(default)]
    username: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct RawChat {
    pub(super) id: i64,
    #[serde(rename = "type")]
    pub(super) kind: String,
}

/// An update, as far as the platform reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedUpdate {
    /// Its number, which names it to tell its retries apart.
    pub update_id: i64,
    pub content: Update,
}

/// What an update brings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// A person's own text message to the bot, in their private chat.
    Message(PrivateMessage),
    /// A person's press of one of the bot's buttons there (ADR-136).
    Press(ButtonPress),
    /// Anything else: taken, ignored.
    Other,
}

/// A person's own message to the bot, in their private chat with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateMessage {
    pub person: TelegramUser,
    /// The message's own number in the chat.
    pub message_id: i64,
    /// Its text, bounded.
    pub text: String,
}

/// A Telegram user, as the platform names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramUser {
    /// Their identifier, which is also their private chat's with the bot.
    pub id: i64,
    /// How they are shown: `@username`, or their name.
    pub name: String,
}

/// Whether `id` is a user's identifier as Telegram gives them.
pub fn is_user_id(id: i64) -> bool {
    (1..=MAX_USER_ID).contains(&id)
}

/// The update `body` brings, or `None` if it is not one Telegram sends.
pub fn read_update(body: &[u8]) -> Option<ReceivedUpdate> {
    let raw: RawUpdate = serde_json::from_slice(body).ok()?;
    if raw.update_id < 0 {
        return None;
    }
    let content = match (raw.message, raw.callback_query) {
        (Some(message), _) => private_message(message).map_or(Update::Other, Update::Message),
        (None, Some(press)) => button_press(press).map_or(Update::Other, Update::Press),
        (None, None) => Update::Other,
    };
    Some(ReceivedUpdate {
        update_id: raw.update_id,
        content,
    })
}

/// The message, if a person wrote it to the bot in their private chat.
fn private_message(message: RawMessage) -> Option<PrivateMessage> {
    let from = message.from?;
    let private = message.chat.kind == "private" && message.chat.id == from.id;
    if !private || from.is_bot || !is_user_id(from.id) {
        return None;
    }
    let text: String = message.text?.chars().take(MAX_MESSAGE_CHARS).collect();
    if text.trim().is_empty() {
        return None;
    }
    Some(PrivateMessage {
        person: TelegramUser {
            id: from.id,
            name: shown_name(&from),
        },
        message_id: message.message_id,
        text,
    })
}

/// `@username` if Telegram gives one as it writes them, else their names,
/// without hidden characters and never starting with `@` (a name anyone
/// chooses must not pass for someone's username), bounded; their identifier
/// at worst.
pub(super) fn shown_name(user: &RawUser) -> String {
    let username = user.username.as_deref().filter(|name| {
        (5..=32).contains(&name.len())
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    });
    if let Some(username) = username {
        return format!("@{username}");
    }
    let names = [Some(user.first_name.as_str()), user.last_name.as_deref()];
    let full: String = names
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|&c| !is_hidden(c))
        .collect();
    let name: String = (full.trim_start_matches(|c: char| c == '@' || c.is_whitespace()))
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    match name.trim() {
        "" => format!("Telegram {}", user.id),
        name => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn update(change: impl FnOnce(&mut Value)) -> Option<ReceivedUpdate> {
        let mut update = json!({
            "update_id": 812,
            "message": {
                "message_id": 7,
                "from": { "id": 4242, "is_bot": false, "first_name": "Léa", "username": "lea_font" },
                "chat": { "id": 4242, "type": "private" },
                "date": 1_700_000_000,
                "text": "Bonjour"
            }
        });
        change(&mut update);
        read_update(update.to_string().as_bytes())
    }

    #[test]
    fn a_person_s_own_message_in_their_private_chat_is_read() {
        let read = update(|_| {}).unwrap();
        assert_eq!(read.update_id, 812);
        let Update::Message(message) = read.content else {
            panic!("a message");
        };
        assert_eq!(message.person.id, 4242);
        assert_eq!(message.person.name, "@lea_font");
        assert_eq!(message.message_id, 7);
        assert_eq!(message.text, "Bonjour");
    }

    #[test]
    fn nothing_else_reaches_the_platform() {
        for change in [
            |u: &mut Value| u["message"]["chat"]["type"] = json!("group"),
            |u: &mut Value| u["message"]["chat"]["id"] = json!(-10_042),
            |u: &mut Value| u["message"]["from"]["is_bot"] = json!(true),
            |u: &mut Value| u["message"]["text"] = json!("   "),
            |u: &mut Value| {
                u["message"].as_object_mut().unwrap().remove("text");
            },
            |u: &mut Value| {
                u["message"].as_object_mut().unwrap().remove("from");
            },
            |u: &mut Value| {
                let message = u["message"].take();
                u["edited_message"] = message;
            },
        ] {
            let read = update(change).unwrap();
            assert_eq!(read.content, Update::Other);
        }
        assert_eq!(read_update(b"pas du json"), None);
        assert_eq!(update(|u| u["update_id"] = json!(-1)), None);
        assert_eq!(update(|u| u["update_id"] = json!("812")), None);
    }

    #[test]
    fn a_name_is_shown_as_telegram_gives_it_bounded() {
        let named = |change: fn(&mut Value)| match update(change).unwrap().content {
            Update::Message(message) => message.person.name,
            Update::Press(_) | Update::Other => panic!("a message"),
        };
        assert_eq!(
            named(|u| {
                u["message"]["from"]["username"] = json!("a b");
                u["message"]["from"]["last_name"] = json!("Font\u{0007}");
            }),
            "Léa Font"
        );
        assert_eq!(
            named(|u| {
                u["message"]["from"]
                    .as_object_mut()
                    .unwrap()
                    .remove("username");
                u["message"]["from"]["first_name"] = json!("");
            }),
            "Telegram 4242"
        );
        // A name never passes for a username, nor hides anything.
        assert_eq!(
            named(|u| {
                u["message"]["from"]
                    .as_object_mut()
                    .unwrap()
                    .remove("username");
                u["message"]["from"]["first_name"] = json!(" @\u{200B}@lea_font\u{202E}");
            }),
            "lea_font"
        );
        assert_eq!(
            named(|u| {
                u["message"]["from"]
                    .as_object_mut()
                    .unwrap()
                    .remove("username");
                u["message"]["from"]["first_name"] = json!("@");
            }),
            "Telegram 4242"
        );
        assert_eq!(
            named(|u| {
                u["message"]["from"]
                    .as_object_mut()
                    .unwrap()
                    .remove("username");
                u["message"]["from"]["first_name"] = json!("é".repeat(100));
            })
            .chars()
            .count(),
            64
        );
        let long = update(|u| u["message"]["text"] = json!("a".repeat(20_000))).unwrap();
        let Update::Message(message) = long.content else {
            panic!("a message");
        };
        assert_eq!(message.text.chars().count(), MAX_MESSAGE_CHARS);
    }
}
