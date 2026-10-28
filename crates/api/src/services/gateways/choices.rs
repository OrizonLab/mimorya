//! A validation given in Slack (ADR-129): the « Valider » or « Refuser »
//! button a linked person pressed under an action of theirs awaiting their
//! validation, sent by Slack, signed, to the gateway's own address
//! (form-encoded, where events are JSON). Answered to Slack at once, then
//! decided in the background as that person, with their rights as they
//! stand, as from the application's screen: Slack is asked first whether the
//! Slack user is still theirs, and only who may decide the request decides
//! it. The message then shows the outcome in place of the buttons; why
//! nothing was decided is said in the thread. Audited as any decision,
//! with the service it was given from.

use mimorya_core::DomainError;
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::slack::{
    ApprovalChoice, DirectMessage, approval_choice, decided_blocks,
};
use mimorya_db::repositories::gateway_identities::linked_account;
use mimorya_db::repositories::gateways::GatewayRow;
use uuid::Uuid;

use super::conversations::{offer_link, say, still_theirs};
use super::{EventOutcome, GatewayService, PersonMessage, bot_token_name};
use crate::app::Services;
use crate::services::access::AuthIdentity;
use crate::services::runs::DecisionInput;

const NOT_DECIDED: &str = "La validation n’a pas pu être enregistrée : réessayez dans un instant, \
     ou donnez-la dans l’application.";
const NOT_LINKED: &str = "Ce compte Slack n’est pas relié à votre compte : rien n’a été décidé.";
const TEMPORARY_PASSWORD: &str = "Votre mot de passe provisoire reste à remplacer dans \
     l’application : rien n’a été décidé.";
const APPROVED: &str = "*Validée* par vous dans Slack.";
const REJECTED: &str = "*Refusée* par vous dans Slack.";
const RESUMED: &str = " La demande reprend : la suite viendra dans ce fil.";

/// A linked person's press of a validation button: to decide once Slack is
/// answered. Never the token, whatever prints it.
pub struct PersonChoice {
    /// Who pressed it and where, as a message of theirs in that thread.
    pub from: PersonMessage,
    pub choice: ApprovalChoice,
}

impl std::fmt::Debug for PersonChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersonChoice")
            .field("gateway", &self.from.gateway.id)
            .field("approval", &self.choice.approval_id)
            .finish_non_exhaustive()
    }
}

impl GatewayService {
    /// What Slack sent the gateway, form-encoded and signed: a press of one
    /// of its buttons, in its workspace, while the gateway is enabled, or
    /// nothing to act upon.
    pub(super) async fn take_choice(
        &self,
        tenant_id: Uuid,
        gateway: GatewayRow,
        body: &[u8],
    ) -> anyhow::Result<EventOutcome> {
        let payload = url::form_urlencoded::parse(body)
            .find(|(key, _)| key == "payload")
            .map(|(_, value)| value.into_owned());
        let Some(choice) = payload.as_deref().and_then(approval_choice) else {
            return Ok(EventOutcome::Taken);
        };
        if !gateway.enabled || choice.team_id != gateway.workspace_id {
            return Ok(EventOutcome::Taken);
        }
        let mut tx = self.db.begin(tenant_id).await?;
        let token = self
            .secret(&mut tx, tenant_id, &bot_token_name(gateway.id))
            .await?;
        tx.commit().await?;
        let Some(token) = token else {
            tracing::warn!(gateway = %gateway.id, "Slack gateway's bot token unreadable");
            return Ok(EventOutcome::Taken);
        };
        let message = DirectMessage {
            person: choice.person.clone(),
            channel: choice.channel.clone(),
            ts: choice.message_ts.clone(),
            thread_ts: choice.thread_ts.clone(),
            text: String::new(),
        };
        let from = PersonMessage {
            tenant_id,
            gateway,
            message,
            token,
        };
        Ok(EventOutcome::Choice(Box::new(PersonChoice {
            from,
            choice,
        })))
    }
}

/// Decides what `pressed` chose, as its person, or says why not.
pub async fn decide_in_slack(services: &Services, pressed: PersonChoice) {
    if let Err(error) = decide(services, &pressed).await {
        let gateway = pressed.from.gateway.id;
        tracing::warn!(%gateway, error = format!("{error:#}"), "Slack validation not decided");
        say(services, &pressed.from, NOT_DECIDED).await;
    }
}

async fn decide(services: &Services, pressed: &PersonChoice) -> anyhow::Result<()> {
    let (from, choice) = (&pressed.from, &pressed.choice);
    let mut tx = services.db.begin(from.tenant_id).await?;
    let account = linked_account(&mut tx, from.gateway.id, &choice.person).await?;
    tx.commit().await?;
    let Some(account) = account else {
        say(services, from, NOT_LINKED).await;
        return offer_link(services, from);
    };
    if account.must_change_password {
        say(services, from, TEMPORARY_PASSWORD).await;
        return Ok(());
    }
    if !still_theirs(services, from, &account).await? {
        return Ok(());
    }
    let identity = AuthIdentity {
        tenant_id: from.tenant_id,
        user_id: account.user_id,
        session_id: Uuid::nil(),
        must_change_password: false,
    };
    let input = DecisionInput {
        approval_id: choice.approval_id,
        approved: choice.approved,
        comment: None,
        gateway: Some(GatewayKind::Slack.as_str()),
    };
    let decided = match services.runs.decide(identity, input).await {
        Ok(decided) => decided,
        Err(error) => {
            let Some(refusal) = error.downcast_ref::<DomainError>() else {
                return Err(error);
            };
            say(services, from, &refusal.message).await;
            return Ok(());
        }
    };
    let mut outcome = (if choice.approved { APPROVED } else { REJECTED }).to_string();
    if decided.resumed {
        outcome.push_str(RESUMED);
    }
    show_outcome(services, pressed, &outcome).await;
    Ok(())
}

/// Shows `outcome` in the message in place of the buttons pressed.
async fn show_outcome(services: &Services, pressed: &PersonChoice, outcome: &str) {
    let (from, choice) = (&pressed.from, &pressed.choice);
    let Some(blocks) = decided_blocks(&choice.blocks, choice.approval_id, outcome) else {
        return;
    };
    let slack = services.gateways.slack();
    if let Err(failure) = slack
        .update(
            &from.token,
            &choice.channel,
            &choice.message_ts,
            outcome,
            &blocks,
        )
        .await
    {
        let failure = failure.message();
        tracing::warn!(gateway = %from.gateway.id, %failure, "Slack message not updated");
    }
}
