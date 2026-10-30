//! A linked person's words to the company's Teams bot (ADR-140), answered
//! in a conversation of theirs, with their rights as they stand, as if
//! written in the application, once they may act (`person::may_act`:
//! still linked, password replaced, company active). The Bot Framework
//! vouches for who wrote: the activity came with the token Microsoft signs,
//! from their personal conversation with the bot, in the company's
//! directory. That conversation holds one of theirs at a time, as a private
//! chat with a Telegram bot does: their message follows the open one, or
//! opens one (`/nouvelle` closed the former, or it was archived in the
//! application); its answers go to the Teams service the latest message
//! came from. Why nothing was started (no model, the agent withdrawn, a
//! request still in progress, the service stopping) is said there. The
//! answer comes when the run ends or pauses (`replies`). What a message
//! says is never logged nor audited. A message taken (answered 200 to the
//! Bot Framework, which does not send it again) is lost, unsaid, should the
//! process vanish before its run is recorded.

use mimorya_core::DomainError;
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::teams::{PersonalMessage, TeamsApplication, escaped};
use mimorya_db::repositories::gateway_identities::linked_account;
use mimorya_db::repositories::gateway_threads::{move_teams_thread, open_gateway_chat};
use uuid::Uuid;

use super::texts;
use crate::app::Services;
use crate::services::gateways::person::{Act, may_act};
use crate::services::gateways::started::{NOT_STARTED, STOPPING, title_of};
use crate::services::runs::{GatewayThread, TurnSender};

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
    if let Err(error) = answer(services, &incoming).await {
        let gateway = incoming.gateway_id;
        tracing::warn!(%gateway, error = format!("{error:#}"), "Teams message not answered");
        say(services, &incoming, NOT_STARTED).await;
    }
}

async fn answer(services: &Services, incoming: &TeamsMessage) -> anyhow::Result<()> {
    let (tenant_id, gateway_id) = (incoming.tenant_id, incoming.gateway_id);
    let message = &incoming.message;
    let person = &message.person.aad_object_id;
    let mut tx = services.db.begin(tenant_id).await?;
    let account = linked_account(&mut tx, gateway_id, person).await?;
    tx.commit().await?;
    // Unlinked since their message was taken.
    let Some(account) = account else {
        say(services, incoming, texts::UNLINKED).await;
        return Ok(());
    };
    let identity = match may_act(services, tenant_id, &account, Act::Start).await? {
        Ok(identity) => identity,
        Err(refusal) => {
            say(services, incoming, &refusal).await;
            return Ok(());
        }
    };
    // Its runs are no longer awaited once the service stops.
    if *services.shutdown.borrow() {
        say(services, incoming, STOPPING).await;
        return Ok(());
    }
    let chat = &message.conversation_id;
    let mut tx = services.db.begin(tenant_id).await?;
    let open = open_gateway_chat(&mut tx, gateway_id, chat, identity.user_id).await?;
    if let Some(held) = &open {
        move_teams_thread(&mut tx, held.id, &message.service_url).await?;
    }
    tx.commit().await?;
    let text = &incoming.words;
    let started = match open {
        Some(held) => {
            let sender = TurnSender::Gateway {
                gateway_id,
                kind: GatewayKind::Teams,
            };
            (services.runs)
                .start_conversation_turn(identity, held.conversation_id, text, sender)
                .await
                .map(drop)
        }
        None => {
            let held = GatewayThread {
                gateway_id,
                kind: GatewayKind::Teams,
                external_id: person,
                channel_id: chat,
                thread_ts: &message.activity_id,
                service_url: Some(&message.service_url),
            };
            (services.runs)
                .start_in_thread(identity, held, &title_of(text), text)
                .await
                .map(drop)
        }
    };
    if let Err(error) = started {
        let Some(refusal) = error.downcast_ref::<DomainError>() else {
            return Err(error);
        };
        say(services, incoming, &refusal.message).await;
    }
    Ok(())
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
