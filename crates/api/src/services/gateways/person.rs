//! Who writes or presses in Slack, before anything is done for them
//! (ADR-127, ADR-129): a Slack user linked to an account of the company, its
//! temporary password replaced, its company active, and Slack still saying
//! it is them, asked each time (no longer so: the identity is unlinked,
//! audited with the reason, and a link to link it again offered). Why not is
//! said in the thread, with what was not done: a request not started, a
//! validation not decided.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_core::gateways::slack::{SlackProfile, escaped, identity_refusal};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::gateway_identities::{
    LinkedAccount, linked_account, unlink_gateway_identity,
};
use serde_json::json;
use uuid::Uuid;

use super::PersonMessage;
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
    if !still_theirs(services, incoming, &account, act).await? {
        return Ok(None);
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
    let (tenant_id, gateway) = (incoming.tenant_id, &incoming.gateway);
    let message = &incoming.message;
    let slack = services.gateways.slack();
    let person = match slack.person(&incoming.token, &message.person).await {
        Ok(person) => person,
        Err(failure) => {
            let failure = failure.message();
            tracing::warn!(gateway = %gateway.id, %failure, "Slack user not verified");
            let unverified = format!(
                "Slack ne dit pas qui vous êtes pour l’instant : {}. Réessayez dans un instant.",
                act.not_done()
            );
            say(services, incoming, &unverified).await;
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
    let unlinked = format!(
        "Slack ne dit plus que ce compte Slack est le vôtre (adresse changée, compte désactivé \
         ou d’un autre espace de travail) : il n’est plus relié à votre compte, et {}.",
        act.not_done()
    );
    say(services, incoming, &unlinked).await;
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
