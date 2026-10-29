//! The company's Telegram bot (ADR-133): what its administrators declare of
//! it (its token), how an update Telegram sends proves it comes from it
//! (the secret the platform gave Telegram with its webhook, sent back in a
//! header), which updates reach the platform (`update`), and the code a
//! person links their Telegram account with: Telegram proves no address, so
//! the code is asked for in the person's own session, opened in Telegram,
//! accepted there by whoever is told whose account it links, and the link
//! confirmed in that session again (`link`). A linked person then talks
//! with the agent there (ADR-135): what their message asks (`command`),
//! and the bot's answers in the HTML Telegram reads (`html`); and validates
//! there what awaits them, with the buttons the bot shows (`buttons`) under
//! the actions a message holds (`shown`) and the press Telegram sends
//! (`press`, ADR-136); a manager, or whom they named to stand in for them,
//! is told there of the requests awaiting them alike (ADR-137).

mod buttons;
mod command;
mod html;
mod link;
mod press;
mod shown;
mod update;

use regex::Regex;
use ring::digest;
use std::sync::LazyLock;

pub use buttons::{
    ApprovalChoice, approval_choice, approval_keyboard, buttons_in, without_decided,
};
pub use command::{HELP, NEW_CONVERSATION, command, new_conversation};
pub use html::{escaped, from_markdown, link};
pub use link::{
    CONSENT, LINK_CODE_CHARS, deep_link, is_bot_username, is_consent, is_link_code, link_code_hash,
    start_code,
};
pub use press::{ButtonPress, KeyboardButton};
pub use shown::{MAX_SHOWN_ACTIONS, shown_actions, shown_in_message};
pub use update::{
    MAX_MESSAGE_CHARS, PrivateMessage, ReceivedUpdate, TelegramUser, Update, is_user_id,
    read_update,
};

/// The header Telegram sends the webhook's secret back in.
pub const SECRET_HEADER: &str = "x-telegram-bot-api-secret-token";
/// The updates the bot asks Telegram for: a message, and a button pressed.
pub const ALLOWED_UPDATES: [&str; 2] = ["message", "callback_query"];
/// How long the webhook's secret the platform makes is.
pub const WEBHOOK_SECRET_CHARS: usize = 64;

/// A bot's token, as BotFather gives it: the bot's id, a colon, its key.
static BOT_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9]{5,20}:[A-Za-z0-9_-]{30,100}$").expect("valid regex"));

/// The bot's token, if it reads as BotFather gives them, or why not (in
/// French).
pub fn bot_token(token: &str) -> Result<String, &'static str> {
    let token = token.trim();
    if BOT_TOKEN.is_match(token) {
        Ok(token.to_owned())
    } else {
        Err(
            "Jeton du bot : celui que BotFather donne au bot, de la forme 123456789:ABC… \
             (son identifiant, deux-points, sa clé)",
        )
    }
}

/// Whether `secret` is a webhook's secret as the platform makes them
/// (Telegram takes 1 to 256 of these characters).
pub fn is_webhook_secret(secret: &str) -> bool {
    secret.len() == WEBHOOK_SECRET_CHARS
        && secret
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Whether the secret Telegram sent back is the webhook's: compared by
/// their digests, so that how long the comparison takes says nothing of
/// the secret.
pub fn secret_matches(expected: &str, sent: Option<&str>) -> bool {
    let Some(sent) = sent else {
        return false;
    };
    let digest_of = |value: &str| digest::digest(&digest::SHA256, value.as_bytes());
    !expected.is_empty() && digest_of(expected).as_ref() == digest_of(sent).as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "123456789:AAH-fictif_jeton_du_bot_de_test_00001";

    #[test]
    fn only_a_bot_s_token_as_botfather_gives_it_is_taken() {
        assert_eq!(bot_token(&format!(" {TOKEN}\n")).as_deref(), Ok(TOKEN));
        for token in [
            "",
            "123456789",
            "123456789:court",
            "abc:AAH-fictif_jeton_du_bot_de_test_00001",
            "123456789:AAH fictif jeton du bot de test 00001",
            "bot123456789:AAH-fictif_jeton_du_bot_de_test_00001",
            "123456789:AAH-fictif_jeton_du_bot_de_test_0000é",
        ] {
            assert!(bot_token(token).is_err(), "{token}");
        }
    }

    #[test]
    fn the_secret_sent_back_must_be_the_webhook_s() {
        let secret = "a".repeat(WEBHOOK_SECRET_CHARS);
        assert!(is_webhook_secret(&secret));
        assert!(!is_webhook_secret(&"a".repeat(63)));
        assert!(!is_webhook_secret(&format!("{}é", "a".repeat(63))));
        assert!(secret_matches(&secret, Some(&secret)));
        assert!(!secret_matches(&secret, Some(&"b".repeat(64))));
        assert!(!secret_matches(&secret, Some("")));
        assert!(!secret_matches(&secret, None));
        assert!(!secret_matches("", Some("")));
    }
}
