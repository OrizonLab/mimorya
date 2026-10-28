//! A linked person's own message to the company's Slack application
//! (ADR-127), answered in a conversation of theirs, with their rights as
//! they stand, as if written in the application, once they may act
//! (`person`: linked, password replaced, company active, Slack asked who
//! the Slack user is today; no longer the person, the identity is unlinked,
//! audited with the reason, and a link to link it again offered). A message
//! outside any thread opens a new conversation, held in the thread it
//! starts; a reply in that thread follows it. Why nothing was started (no
//! model, the agent withdrawn, a request still in progress, the service
//! stopping) is said in the thread. The answer comes when the run ends or
//! pauses (`replies`). What a message says is never logged nor audited. A
//! message taken (answered 200 to Slack, which does not send it again) is
//! lost, unsaid, should the process vanish before its run is recorded.

use mimorya_core::DomainError;
use mimorya_core::gateways::slack::plain;
use mimorya_db::repositories::gateway_threads::find_gateway_thread;

use super::PersonMessage;
use super::person::{Act, acting_person, say, thread_of};
use crate::app::Services;
use crate::services::runs::{SlackThread, TITLE_LENGTH, TurnSender};

const NOT_STARTED: &str = "La demande n’a pas pu être lancée : réessayez dans un instant.";
const OTHER_PERSON_THREAD: &str = "Ce fil tient la conversation d’un autre compte : écrivez un \
     nouveau message, hors de ce fil.";
const STOPPING: &str = "Le service redémarre : rien n’a été lancé. Réessayez dans un instant.";

/// Answers `incoming` in a conversation of its author's, or says why not.
pub async fn converse(services: &Services, incoming: PersonMessage) {
    if let Err(error) = answer(services, &incoming).await {
        let gateway = incoming.gateway.id;
        tracing::warn!(%gateway, error = format!("{error:#}"), "Slack message not answered");
        say(services, &incoming, NOT_STARTED).await;
    }
}

async fn answer(services: &Services, incoming: &PersonMessage) -> anyhow::Result<()> {
    let gateway_id = incoming.gateway.id;
    let message = &incoming.message;
    let Some(identity) = acting_person(services, incoming, Act::Start).await? else {
        return Ok(());
    };
    // Its runs are no longer awaited once the service stops.
    if *services.shutdown.borrow() {
        say(services, incoming, STOPPING).await;
        return Ok(());
    }
    let thread_ts = thread_of(incoming);
    let text = plain(&message.text);
    let mut tx = services.db.begin(identity.tenant_id).await?;
    let held = find_gateway_thread(&mut tx, gateway_id, &message.channel, thread_ts).await?;
    tx.commit().await?;
    let started = match held {
        Some(thread) if thread.user_id == identity.user_id => {
            let sender = TurnSender::Gateway { gateway_id };
            (services.runs)
                .start_conversation_turn(identity, thread.conversation_id, &text, sender)
                .await
                .map(drop)
        }
        // Their identity was linked to another account since.
        Some(_) => Err(DomainError::conflict(OTHER_PERSON_THREAD).into()),
        None => {
            // On one line, however it was written.
            let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let title: String = words.chars().take(TITLE_LENGTH).collect();
            let thread = SlackThread {
                gateway_id,
                external_id: &message.person,
                channel_id: &message.channel,
                thread_ts,
            };
            (services.runs)
                .start_in_thread(identity, thread, &title, &text)
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
