//! A linked person's words in their private conversation with a company's
//! bot, answered alike in Telegram (ADR-135) and in Teams (ADR-140): in a
//! conversation of theirs, with their rights as they stand, as if written
//! in the application, once they may act (`person::may_act`: still linked,
//! password replaced, company active). That private conversation holds one
//! conversation at a time: their message follows the open one, or opens
//! one (`/nouvelle` closed the former, or it was archived in the
//! application). What becomes of it is said by each service in its own
//! way: nothing once started (the answer comes when the run ends or pauses,
//! `replies`), or why nothing was (unlinked meanwhile, no model, the agent
//! withdrawn, a request still in progress, the service stopping).

use mimorya_core::DomainError;
use mimorya_core::gateways::GatewayKind;
use mimorya_db::repositories::gateway_identities::linked_account;
use mimorya_db::repositories::gateway_threads::open_gateway_chat;
use uuid::Uuid;

use super::person::{Act, may_act};
use super::started::{STOPPING, title_of};
use crate::app::Services;
use crate::services::runs::{GatewayThread, TurnSender};

/// A linked person's words in their private conversation with a bot.
pub(super) struct ChatWords<'a> {
    pub tenant_id: Uuid,
    pub gateway_id: Uuid,
    pub kind: GatewayKind,
    /// Who wrote, as the service names them.
    pub person: &'a str,
    /// Their private conversation with the bot.
    pub chat: &'a str,
    /// Their message, which opens a conversation when none is open.
    pub message_id: &'a str,
    /// The Teams service it came from (Teams only).
    pub service_url: Option<&'a str>,
    /// Their words for the agent.
    pub text: &'a str,
}

/// Answers `words` in a conversation of its author's: what to say instead,
/// if nothing was started; `unlinked`, should they be unlinked meanwhile.
pub(super) async fn converse_in_chat(
    services: &Services,
    words: ChatWords<'_>,
    unlinked: &str,
) -> anyhow::Result<Option<String>> {
    let (tenant_id, gateway_id) = (words.tenant_id, words.gateway_id);
    let mut tx = services.db.begin(tenant_id).await?;
    let account = linked_account(&mut tx, gateway_id, words.person).await?;
    tx.commit().await?;
    // Unlinked since their message was taken.
    let Some(account) = account else {
        return Ok(Some(unlinked.to_owned()));
    };
    let identity = match may_act(services, tenant_id, &account, Act::Start).await? {
        Ok(identity) => identity,
        Err(refusal) => return Ok(Some(refusal)),
    };
    // Its runs are no longer awaited once the service stops.
    if *services.shutdown.borrow() {
        return Ok(Some(STOPPING.to_owned()));
    }
    let mut tx = services.db.begin(tenant_id).await?;
    let open = open_gateway_chat(&mut tx, gateway_id, words.chat, identity.user_id).await?;
    tx.commit().await?;
    let text = words.text;
    let started = match open {
        Some(held) => {
            let sender = TurnSender::Gateway {
                gateway_id,
                kind: words.kind,
            };
            (services.runs)
                .start_conversation_turn(identity, held.conversation_id, text, sender)
                .await
                .map(drop)
        }
        None => {
            let held = GatewayThread {
                gateway_id,
                kind: words.kind,
                external_id: words.person,
                channel_id: words.chat,
                thread_ts: words.message_id,
                service_url: words.service_url,
            };
            (services.runs)
                .start_in_thread(identity, held, &title_of(text), text)
                .await
                .map(drop)
        }
    };
    match started {
        Ok(()) => Ok(None),
        Err(error) => match error.downcast_ref::<DomainError>() {
            Some(refusal) => Ok(Some(refusal.message.clone())),
            None => Err(error),
        },
    }
}
