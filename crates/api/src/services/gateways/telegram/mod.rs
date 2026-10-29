//! The company's Telegram bot as its gateway (ADR-133): the updates
//! Telegram sends it, proven by the webhook's secret alone (`updates`), and
//! the link a person makes between their Telegram account and theirs: a
//! code asked for in their session, sent back by Telegram from their
//! private chat with the bot, confirmed in their session again (`link`).

mod link;
mod texts;
mod updates;

pub use link::{PendingLink, TelegramLinkCode};
pub use updates::UpdateOutcome;
