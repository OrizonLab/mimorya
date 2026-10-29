//! The company's Telegram bot as its gateway (ADR-133): the updates
//! Telegram sends it, proven by the webhook's secret alone (`updates`), and
//! the link a person makes between their Telegram account and theirs: a
//! code asked for in their session, sent back by Telegram from their
//! private chat with the bot, accepted there by whoever the bot told whose
//! account it links (`answers`), confirmed in their session again (`link`).
//! A linked person's words are then answered by the agent in a
//! conversation of theirs (`conversations`, ADR-135), and the run's end or
//! pause written in their chat (`reply_text`, by `replies`); what awaits
//! their own validation there is validated with the buttons under the
//! pause (`choices`, ADR-136).

mod answers;
mod choices;
mod conversations;
mod link;
mod reply_text;
mod texts;
mod updates;

pub use choices::{TelegramChoice, decide_in_telegram};
pub use conversations::converse_in_telegram;
pub use link::{PendingLink, TelegramLinkCode};
pub(super) use reply_text::{Markup, Pause, reply_text};
pub use updates::{ChatMessage, UpdateOutcome};
