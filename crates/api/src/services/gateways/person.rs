//! Who writes or presses in Slack, before anything is done for them
//! (ADR-127, ADR-129): in their own direct conversation with the
//! application, as Slack says (ADR-131: another's, nothing said there and
//! audited; Slack not saying, the sender told in their own), a Slack user
//! linked to an account of the company,
//! Slack still saying it is them, asked each time before anything is said of
//! the account (no longer so: the identity is unlinked, audited with the
//! reason, and a link to link it again offered), its temporary password
//! replaced, its company active. Why not is said in the thread, with what
//! was not done: a request not started, a validation not decided.

use chrono::Utc;
use mimorya_connectors::slack::SlackFailure;
use mimorya_core::DomainError;
use mimorya_core::gateways::slack::{SlackProfile, escaped, identity_refusal};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_identities::{
    LinkedAccount, linked_account, unlink_gateway_identity,
};
use mimorya_db::repositories::gateways::GatewayRow;
use serde_json::json;
use uuid::Uuid;

use super::PersonMessage;
use super::direct::{Whose, whose_conversation};
use crate::app::Services;
use crate::services::access::AuthIdentity;
use crate::services::runs::COMPANY_NOT_ACTIVE;

/// What the person's message or press would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Act {
    /// Start a request (ADR-127).
    Start,
    /// Decide a validation (ADR-129).
    Decide,
}

impl Act {
    /// What was not done, said when the person may not act.
    fn not_done(self) -> &'static str {
        match self {
            Self::Start => "rien n’a été lancé",
            Self::Decide => "rien n’a été décidé",
        }
    }
}

/// Whom `incoming` comes from, if they may act now; otherwise why not is
/// said in its thread (a Slack user linked to no account, unless it pressed
/// a button, is offered a link without a word, as anyone writing first).
pub(super) async fn acting_person(
    services: &Services,
    incoming: &PersonMessage,
    act: Act,
) -> anyhow::Result<Option<AuthIdentity>> {
    let tenant_id = incoming.tenant_id;
    // Nothing said, nor done, where it could be read by another.
    let message = &incoming.message;
    let gateway = (incoming.gateway.id, incoming.token.as_str());
    let slack = services.gateways.slack();
    match whose_conversation(slack, gateway, &message.channel, &message.person).await {
        Whose::Theirs => {}
        Whose::Another => {
            // Audited if it can be; never anything said there, whatever fails.
            if let Err(error) = refuse_conversation(services, incoming).await {
                let gateway = incoming.gateway.id;
                let error = format!("{error:#}");
                tracing::warn!(%gateway, %error, "Refused Slack conversation not audited");
            }
            return Ok(None);
        }
        Whose::Unsaid(failure) => {
            tell_unverified(services, incoming, act, &failure).await;
            return Ok(None);
        }
    }
    let mut tx = services.db.begin(tenant_id).await?;
    let account = linked_account(&mut tx, incoming.gateway.id, &incoming.message.person).await?;
    tx.commit().await?;
    let Some(account) = account else {
        if act == Act::Decide {
            let unlinked = format!(
                "Ce compte Slack n’est pas relié à votre compte : {}.",
                act.not_done()
            );
            say(services, incoming, &unlinked).await;
        }
        offer_link(services, incoming)?;
        return Ok(None);
    };
    // Nothing said of the account before Slack says the Slack user is its
    // person.
    if !still_theirs(services, incoming, &account, act).await? {
        return Ok(None);
    }
    if account.must_change_password {
        let temporary = format!(
            "Votre mot de passe provisoire reste à remplacer dans l’application : {}.",
            act.not_done()
        );
        say(services, incoming, &temporary).await;
        return Ok(None);
    }
    let identity = AuthIdentity {
        tenant_id,
        user_id: account.user_id,
        session_id: Uuid::nil(),
        must_change_password: false,
    };
    match services.access.context(identity).await {
        Ok(access) if access.tenant.status == "active" => {}
        Ok(_) => {
            say(services, incoming, COMPANY_NOT_ACTIVE).await;
            return Ok(None);
        }
        Err(error) => {
            let Some(refusal) = error.downcast_ref::<DomainError>() else {
                return Err(error);
            };
            say(services, incoming, &refusal.message).await;
            return Ok(None);
        }
    }
    Ok(Some(identity))
}

/// Whether Slack still says the Slack user is the person of `account`
/// (ADR-125). Once it no longer does, their identity is unlinked and a new
/// link offered; when Slack does not say, nothing is done.
async fn still_theirs(
    services: &Services,
    incoming: &PersonMessage,
    account: &LinkedAccount,
    act: Act,
) -> anyhow::Result<bool> {
    let linked = (incoming.message.person.as_str(), account.user_id);
    let verdict = slack_says_theirs(
        services,
        incoming.tenant_id,
        (&incoming.gateway, &incoming.token),
        linked,
        &account.email,
    )
    .await?;
    let not_done = act.not_done();
    match verdict {
        Ok(true) => Ok(true),
        Ok(false) => {
            let unlinked = format!(
                "Slack ne dit plus que ce compte Slack est le vôtre (adresse changée, compte \
                 désactivé ou d’un autre espace de travail) : il n’est plus relié à votre \
                 compte, et {not_done}."
            );
            say(services, incoming, &unlinked).await;
            offer_link(services, incoming)?;
            Ok(false)
        }
        Err(_) => {
            let unverified = format!(
                "Slack ne dit pas qui vous êtes pour l’instant : {not_done}. Réessayez dans un \
                 instant."
            );
            say(services, incoming, &unverified).await;
            Ok(false)
        }
    }
}

/// Whether Slack, asked with the gateway's `token`, still says the Slack
/// user `external_id` is the person `user_id`, of address `email`, of its
/// workspace (ADR-125). Once it no longer does, their identity is unlinked,
/// audited with the reason: `Ok(false)`. `Err` when Slack does not say.
pub(super) async fn slack_says_theirs(
    services: &Services,
    tenant_id: Uuid,
    (gateway, token): (&GatewayRow, &str),
    (external_id, user_id): (&str, Uuid),
    email: &str,
) -> anyhow::Result<Result<bool, SlackFailure>> {
    let slack = services.gateways.slack();
    let person = match slack.person(token, external_id).await {
        Ok(person) => person,
        Err(failure) => {
            let reason = failure.message();
            tracing::warn!(gateway = %gateway.id, %reason, "Slack user not verified");
            return Ok(Err(failure));
        }
    };
    let profile = SlackProfile {
        workspace: person.workspace_id.as_deref(),
        address: person.address.as_deref(),
        deleted: person.deleted,
        bot: person.bot,
    };
    let Some(refusal) = identity_refusal(&profile, &gateway.workspace_id, email) else {
        return Ok(Ok(true));
    };
    let mut tx = services.db.begin(tenant_id).await?;
    if unlink_gateway_identity(&mut tx, gateway.id, external_id, user_id).await? {
        let entry = AuditEntry::new(tenant_id, "system", "gateway.identity_unlinked", "success")
            .on_behalf_of(user_id)
            .resource("gateway", gateway.id)
            .metadata(json!({ "externalId": external_id, "reason": refusal.code() }));
        append_audit(&mut tx, entry).await?;
    }
    tx.commit().await?;
    Ok(Ok(false))
}

/// Audits, for the company's administrators, a message or a press naming
/// a conversation that is not its sender's own: whoever sent it holds the
/// signing secret. Never what it says, nor where.
async fn refuse_conversation(services: &Services, incoming: &PersonMessage) -> anyhow::Result<()> {
    let tenant_id = incoming.tenant_id;
    let mut tx = services.db.begin(tenant_id).await?;
    let entry = AuditEntry::new(
        tenant_id,
        "system",
        "gateway.conversation_refused",
        "denied",
    )
    .resource("gateway", incoming.gateway.id)
    .metadata(json!({ "externalId": incoming.message.person }));
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

/// Tells the sender, in their own direct conversation with the application
/// (found by Slack from their identifier, never the one the event names),
/// that Slack did not say whose this one is, and that nothing was done.
async fn tell_unverified(
    services: &Services,
    incoming: &PersonMessage,
    act: Act,
    failure: &SlackFailure,
) {
    let not_done = act.not_done();
    let text = if matches!(failure, SlackFailure::Refused(code) if code == "missing_scope") {
        format!(
            "L’application Slack de l’entreprise n’a pas la permission im:read, qui lui fait \
             dire si une conversation est la vôtre : {not_done}. Demandez à votre \
             administrateur de la lui ajouter, puis de la réinstaller."
        )
    } else {
        format!(
            "Slack ne dit pas pour l’instant si cette conversation est la vôtre : {not_done}. \
             Réessayez dans un instant."
        )
    };
    let slack = services.gateways.slack();
    let person = &incoming.message.person;
    if let Err(failure) = slack
        .post(&incoming.token, person, None, &escaped(&text))
        .await
    {
        let failure = failure.message();
        tracing::warn!(gateway = %incoming.gateway.id, %failure, "Slack answer not sent");
    }
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
pub(super) fn thread_of(incoming: &PersonMessage) -> &str {
    let message = &incoming.message;
    message.thread_ts.as_deref().unwrap_or(&message.ts)
}

/// Says `text` to the person, in the thread of their message.
pub(super) async fn say(services: &Services, incoming: &PersonMessage, text: &str) {
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
