//! What a linked person's message to the company's Teams bot asks
//! (ADR-140), read as in Telegram (`command`): `/nouvelle` closes their
//! conversation held in their personal conversation with the bot (the
//! words that follow it, if any, open the next one); a button of a card,
//! pressed, is a choice to decide (ADR-141); another command, or a message
//! without words (a file), says to whose account theirs is linked and how
//! to talk with the assistant; anything else is words for the agent.

use mimorya_core::gateways::teams::CardChoice;
use mimorya_core::gateways::{command, new_conversation};
use mimorya_db::TenantTx;
use mimorya_db::repositories::gateway_threads::close_gateway_chat;
use mimorya_db::repositories::users::get_user;
use uuid::Uuid;

use super::texts;

/// What becomes of a linked person's message.
pub(super) enum Answer {
    /// The bot says this, in plain French.
    Say(String),
    /// These words go to the agent.
    Converse(String),
    /// A card's button pressed: this choice is to decide.
    Choose(CardChoice),
}

/// What the message `text` (or the card's button `choice` pressed) of the
/// person `user_id`, in their personal conversation `conversation_id` with
/// the bot `gateway_id`, asks; the assistant is named `assistant`.
pub(super) async fn answer_to(
    tx: &mut TenantTx,
    (gateway_id, user_id): (Uuid, Uuid),
    conversation_id: &str,
    (text, choice): (&str, Option<CardChoice>),
    assistant: &str,
) -> anyhow::Result<Answer> {
    if let Some(choice) = choice {
        return Ok(Answer::Choose(choice));
    }
    if let Some(first) = new_conversation(text) {
        close_gateway_chat(tx, gateway_id, conversation_id, user_id).await?;
        return Ok(if first.is_empty() {
            Answer::Say(texts::NEW_CONVERSATION_NEXT.to_owned())
        } else {
            Answer::Converse(first.to_owned())
        });
    }
    if command(text).is_some() || text.trim().is_empty() {
        let user = get_user(tx, user_id).await?;
        let name = user.map_or_else(String::new, |user| user.display_name);
        return Ok(Answer::Say(texts::help(assistant, &name)));
    }
    Ok(Answer::Converse(text.to_owned()))
}
