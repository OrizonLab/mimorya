//! A linked person's words to the company's Teams bot (ADR-140), answered
//! in a conversation of theirs held in their personal conversation with the
//! bot, as in Telegram (`private_chats`); its answers go to the Teams
//! service the latest message came from (`activities`). The Bot Framework
//! vouches for who wrote: the activity came with the token Microsoft signs,
//! from their personal conversation with the bot, in the company's
//! directory. Why nothing was started is said there; the answer comes when
//! the run ends or pauses (`replies`). What a message says is never logged
//! nor audited. A message taken (answered 200 to the Bot Framework, which
//! does not send it again) is lost, unsaid, should the process vanish
//! before its run is recorded.

use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::teams::{PersonalMessage, TeamsApplication, escaped};
use uuid::Uuid;

use super::texts;
use crate::app::Services;
use crate::services::gateways::private_chats::{ChatWords, converse_in_chat};
use crate::services::gateways::started::NOT_STARTED;

/// A linked person's words to the bot, to answer.
pub struct TeamsMessage {
    pub tenant_id: Uuid,
    pub gateway_id: Uuid,
    /// The bot's application, to answer with.
    pub app: TeamsApplication,
    pub message: PersonalMessage,
    /// Their words for the agent.
    pub words: String,
}

/// Never the words nor the client secret, whatever prints it.
impl std::fmt::Debug for TeamsMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeamsMessage")
            .field("tenant_id", &self.tenant_id)
            .field("gateway_id", &self.gateway_id)
            .field("activity_id", &self.message.activity_id)
            .finish_non_exhaustive()
    }
}

/// Answers `incoming` in a conversation of its author's, or says why not.
pub async fn converse_in_teams(services: &Services, incoming: TeamsMessage) {
    let message = &incoming.message;
    let words = ChatWords {
        tenant_id: incoming.tenant_id,
        gateway_id: incoming.gateway_id,
        kind: GatewayKind::Teams,
        person: &message.person.aad_object_id,
        chat: &message.conversation_id,
        message_id: &message.activity_id,
        service_url: Some(&message.service_url),
        text: &incoming.words,
    };
    let said = match converse_in_chat(services, words, texts::UNLINKED).await {
        Ok(said) => said,
        Err(error) => {
            let gateway = incoming.gateway_id;
            tracing::warn!(%gateway, error = format!("{error:#}"), "Teams message not answered");
            Some(NOT_STARTED.to_owned())
        }
    };
    if let Some(text) = said {
        say(services, &incoming, &text).await;
    }
}

/// Says `text` (plain French) to the person, in their personal
/// conversation with the bot.
async fn say(services: &Services, incoming: &TeamsMessage, text: &str) {
    let message = &incoming.message;
    let to = (
        message.service_url.as_str(),
        message.conversation_id.as_str(),
    );
    let teams = services.gateways.teams();
    if let Err(failure) = teams.send(&incoming.app, to, &escaped(text)).await {
        let failure = failure.message();
        tracing::warn!(gateway = %incoming.gateway_id, %failure, "Teams answer not sent");
    }
}
