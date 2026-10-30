//! A linked person's words to the company's Telegram bot (ADR-135),
//! answered in a conversation of theirs held in their private chat with
//! the bot, as in Teams (`private_chats`). Telegram vouches for who wrote:
//! the update came with the webhook's secret, from their private chat with
//! the bot. Why nothing was started is said in the chat, in plain text; the
//! answer comes when the run ends or pauses (`replies`). What a message
//! says is never logged nor audited. A message taken (answered 200 to
//! Telegram, which does not send it again) is lost, unsaid, should the
//! process vanish before its run is recorded.

use mimorya_core::gateways::GatewayKind;

use super::ChatMessage;
use super::texts;
use crate::app::Services;
use crate::services::gateways::private_chats::{ChatWords, converse_in_chat};
use crate::services::gateways::started::NOT_STARTED;

/// Answers `incoming` in a conversation of its author's, or says why not.
pub async fn converse_in_telegram(services: &Services, incoming: ChatMessage) {
    let person = incoming.person.to_string();
    let opening = incoming.message_id.to_string();
    let words = ChatWords {
        tenant_id: incoming.tenant_id,
        gateway_id: incoming.gateway_id,
        kind: GatewayKind::Telegram,
        person: &person,
        // Their private chat is named as they are.
        chat: &person,
        message_id: &opening,
        service_url: None,
        text: &incoming.text,
    };
    let unlinked = texts::unlinked(&services.gateways.assistant_name);
    let said = match converse_in_chat(services, words, &unlinked).await {
        Ok(said) => said,
        Err(error) => {
            let gateway = incoming.gateway_id;
            tracing::warn!(%gateway, error = format!("{error:#}"), "Telegram message not answered");
            Some(NOT_STARTED.to_owned())
        }
    };
    if let Some(text) = said {
        say(services, &incoming, &text).await;
    }
}

/// Says `text` to the person, in their private chat with the bot.
async fn say(services: &Services, incoming: &ChatMessage, text: &str) {
    let telegram = services.gateways.telegram();
    if let Err(failure) = telegram.send(&incoming.token, incoming.person, text).await {
        let failure = failure.message();
        tracing::warn!(gateway = %incoming.gateway_id, %failure, "Telegram answer not sent");
    }
}
