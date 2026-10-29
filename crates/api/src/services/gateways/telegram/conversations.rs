//! A linked person's words to the company's Telegram bot (ADR-135),
//! answered in a conversation of theirs, with their rights as they stand,
//! as if written in the application, once they may act (`person::may_act`:
//! still linked, password replaced, company active). Telegram vouches for
//! who wrote: the update came with the webhook's secret, from their private
//! chat with the bot. That chat holds one conversation at a time: their
//! message follows the open one, or opens one (`/nouvelle` closed the
//! former, or it was archived in the application). Why nothing was started
//! (no model, the agent withdrawn, a request still in progress, the service
//! stopping) is said in the chat. The answer comes when the run ends or
//! pauses (`replies`). What a message says is never logged nor audited. A
//! message taken (answered 200 to Telegram, which does not send it again)
//! is lost, unsaid, should the process vanish before its run is recorded.

use mimorya_core::DomainError;
use mimorya_core::gateways::GatewayKind;
use mimorya_db::repositories::gateway_identities::linked_account;
use mimorya_db::repositories::gateway_threads::open_gateway_chat;

use super::ChatMessage;
use super::texts;
use crate::app::Services;
use crate::services::gateways::person::{Act, may_act};
use crate::services::gateways::started::{NOT_STARTED, STOPPING, title_of};
use crate::services::runs::{GatewayThread, TurnSender};

/// Answers `incoming` in a conversation of its author's, or says why not.
pub async fn converse_in_telegram(services: &Services, incoming: ChatMessage) {
    if let Err(error) = answer(services, &incoming).await {
        let gateway = incoming.gateway_id;
        tracing::warn!(%gateway, error = format!("{error:#}"), "Telegram message not answered");
        say(services, &incoming, NOT_STARTED).await;
    }
}

async fn answer(services: &Services, incoming: &ChatMessage) -> anyhow::Result<()> {
    let (tenant_id, gateway_id) = (incoming.tenant_id, incoming.gateway_id);
    let person = incoming.person.to_string();
    let mut tx = services.db.begin(tenant_id).await?;
    let account = linked_account(&mut tx, gateway_id, &person).await?;
    tx.commit().await?;
    // Unlinked since their message was taken.
    let Some(account) = account else {
        let unlinked = texts::unlinked(&services.gateways.assistant_name);
        say(services, incoming, &unlinked).await;
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
    let mut tx = services.db.begin(tenant_id).await?;
    let open = open_gateway_chat(&mut tx, gateway_id, &person, identity.user_id).await?;
    tx.commit().await?;
    let text = &incoming.text;
    let started = match open {
        Some(chat) => {
            let sender = TurnSender::Gateway {
                gateway_id,
                kind: GatewayKind::Telegram,
            };
            (services.runs)
                .start_conversation_turn(identity, chat.conversation_id, text, sender)
                .await
                .map(drop)
        }
        None => {
            let opening = incoming.message_id.to_string();
            let chat = GatewayThread {
                gateway_id,
                kind: GatewayKind::Telegram,
                external_id: &person,
                channel_id: &person,
                thread_ts: &opening,
            };
            (services.runs)
                .start_in_thread(identity, chat, &title_of(text), text)
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

/// Says `text` to the person, in their private chat with the bot.
async fn say(services: &Services, incoming: &ChatMessage, text: &str) {
    let telegram = services.gateways.telegram();
    if let Err(failure) = telegram.send(&incoming.token, incoming.person, text).await {
        let failure = failure.message();
        tracing::warn!(gateway = %incoming.gateway_id, %failure, "Telegram answer not sent");
    }
}
