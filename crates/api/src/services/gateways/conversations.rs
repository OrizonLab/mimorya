//! A linked person's own message to the company's Slack application
//! (ADR-127), answered in a conversation of theirs, with their rights as
//! they stand, as if written in the application. Slack is asked first who
//! the Slack user is today: no longer the person (address changed,
//! deactivated, of another workspace), the identity is unlinked, audited
//! with the reason, and a link to link it again is offered. A message
//! outside any thread opens a new conversation, held in the thread it
//! starts; a reply in that thread follows it. Why nothing was started (no
//! model, the agent withdrawn, a request still in progress, the service
//! stopping) is said in the thread. The answer comes when the run ends or
//! pauses (`replies`). What a message says is never logged nor audited. A
//! message taken (answered 200 to Slack, which does not send it again) is
//! lost, unsaid, should the process vanish before its run is recorded.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_core::gateways::slack::{SlackProfile, escaped, identity_refusal, plain};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_identities::{
    LinkedAccount, linked_account, unlink_gateway_identity,
};
use mimorya_db::repositories::gateway_threads::find_gateway_thread;
use serde_json::json;
use uuid::Uuid;

use super::PersonMessage;
use crate::app::Services;
use crate::services::access::AuthIdentity;
use crate::services::runs::{SlackThread, TITLE_LENGTH, TurnSender};

const NOT_STARTED: &str = "La demande n’a pas pu être lancée : réessayez dans un instant.";
const UNVERIFIED: &str = "Slack ne dit pas qui vous êtes pour l’instant : rien n’a été lancé. \
     Réessayez dans un instant.";
const TEMPORARY_PASSWORD: &str = "Votre mot de passe provisoire reste à remplacer dans \
     l’application : rien n’a été lancé.";
const OTHER_PERSON_THREAD: &str = "Ce fil tient la conversation d’un autre compte : écrivez un \
     nouveau message, hors de ce fil.";
const STOPPING: &str = "Le service redémarre : rien n’a été lancé. Réessayez dans un instant.";
const UNLINKED: &str = "Slack ne dit plus que ce compte Slack est le vôtre (adresse changée, \
     compte désactivé ou d’un autre espace de travail) : il n’est plus relié à votre compte, et \
     rien n’a été lancé.";

/// Answers `incoming` in a conversation of its author's, or says why not.
pub async fn converse(services: &Services, incoming: PersonMessage) {
    if let Err(error) = answer(services, &incoming).await {
        let gateway = incoming.gateway.id;
        tracing::warn!(%gateway, error = format!("{error:#}"), "Slack message not answered");
        say(services, &incoming, NOT_STARTED).await;
    }
}

async fn answer(services: &Services, incoming: &PersonMessage) -> anyhow::Result<()> {
    let (tenant_id, gateway_id) = (incoming.tenant_id, incoming.gateway.id);
    let message = &incoming.message;
    let mut tx = services.db.begin(tenant_id).await?;
    let account = linked_account(&mut tx, gateway_id, &message.person).await?;
    tx.commit().await?;
    // Unlinked meanwhile: as someone who never linked.
    let Some(account) = account else {
        return offer_link(services, incoming);
    };
    if account.must_change_password {
        say(services, incoming, TEMPORARY_PASSWORD).await;
        return Ok(());
    }
    if !still_theirs(services, incoming, &account).await? {
        return Ok(());
    }

    let identity = AuthIdentity {
        tenant_id,
        user_id: account.user_id,
        session_id: Uuid::nil(),
        must_change_password: false,
    };
    // Its runs are no longer awaited once the service stops.
    if *services.shutdown.borrow() {
        say(services, incoming, STOPPING).await;
        return Ok(());
    }
    let thread_ts = thread_of(incoming);
    let text = plain(&message.text);
    let mut tx = services.db.begin(tenant_id).await?;
    let held = find_gateway_thread(&mut tx, gateway_id, &message.channel, thread_ts).await?;
    tx.commit().await?;
    let started = match held {
        Some(thread) if thread.user_id == account.user_id => {
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

/// Whether Slack still says the Slack user is the person of `account`
/// (ADR-125): asked at every message. Once it no longer does, their
/// identity is unlinked and a new link offered; when Slack does not say,
/// nothing is started.
async fn still_theirs(
    services: &Services,
    incoming: &PersonMessage,
    account: &LinkedAccount,
) -> anyhow::Result<bool> {
    let (tenant_id, gateway) = (incoming.tenant_id, &incoming.gateway);
    let message = &incoming.message;
    let slack = services.gateways.slack();
    let person = match slack.person(&incoming.token, &message.person).await {
        Ok(person) => person,
        Err(failure) => {
            let failure = failure.message();
            tracing::warn!(gateway = %gateway.id, %failure, "Slack user not verified");
            say(services, incoming, UNVERIFIED).await;
            return Ok(false);
        }
    };
    let profile = SlackProfile {
        workspace: person.workspace_id.as_deref(),
        address: person.address.as_deref(),
        deleted: person.deleted,
        bot: person.bot,
    };
    let Some(refusal) = identity_refusal(&profile, &gateway.workspace_id, &account.email) else {
        return Ok(true);
    };
    let mut tx = services.db.begin(tenant_id).await?;
    if unlink_gateway_identity(&mut tx, gateway.id, &message.person, account.user_id).await? {
        let entry = AuditEntry::new(tenant_id, "system", "gateway.identity_unlinked", "success")
            .on_behalf_of(account.user_id)
            .resource("gateway", gateway.id)
            .metadata(json!({ "externalId": message.person, "reason": refusal.code() }));
        append_audit(&mut tx, entry).await?;
    }
    tx.commit().await?;
    say(services, incoming, UNLINKED).await;
    offer_link(services, incoming)?;
    Ok(false)
}

fn offer_link(services: &Services, incoming: &PersonMessage) -> anyhow::Result<()> {
    services.gateways.offer_link(
        incoming.tenant_id,
        incoming.gateway.id,
        incoming.token.clone(),
        incoming.message.clone(),
        Utc::now(),
    )
}

/// The thread the message is in, or the one it starts.
fn thread_of(incoming: &PersonMessage) -> &str {
    let message = &incoming.message;
    message.thread_ts.as_deref().unwrap_or(&message.ts)
}

/// Says `text` to the person, in the thread of their message.
async fn say(services: &Services, incoming: &PersonMessage, text: &str) {
    let slack = services.gateways.slack();
    let channel = &incoming.message.channel;
    let thread = Some(thread_of(incoming));
    if let Err(failure) = slack
        .post(&incoming.token, channel, thread, &escaped(text))
        .await
    {
        let failure = failure.message();
        tracing::warn!(gateway = %incoming.gateway.id, %failure, "Slack answer not sent");
    }
}
