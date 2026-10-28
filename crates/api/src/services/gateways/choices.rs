//! A validation given in Slack (ADR-129): the « Valider » or « Refuser »
//! button a linked person pressed under an action of theirs awaiting their
//! validation, sent by Slack, signed, to the gateway's own address
//! (form-encoded, where events are JSON). Answered to Slack at once, then
//! decided in the background as that person, once they may act (`person`),
//! with their rights as they stand, as from the application's screen. Only
//! their own action, awaiting their own validation, that Slack may validate
//! (`validated_in_slack`: changing nothing outside the company, undone, its
//! exact parameters shown whole) is decided there: whatever the press says,
//! any other is left to the application. The message then shows what
//! became of each of its actions in place of their buttons; why nothing was
//! decided is said in the thread. Audited as any decision, with the service
//! it was given from.

use mimorya_core::DomainError;
use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::slack::{
    ApprovalChoice, DirectMessage, approval_choice, buttons_in, decided_blocks, validated_in_slack,
};
use mimorya_db::repositories::approvals::{ApprovalRow, get_approval};
use mimorya_db::repositories::gateways::GatewayRow;
use uuid::Uuid;

use super::person::{Act, acting_person, say};
use super::{EventOutcome, GatewayService, PersonMessage, bot_token_name};
use crate::app::Services;
use crate::services::access::AuthIdentity;
use crate::services::runs::{APPROVAL_NOT_FOUND, DecisionInput};

const NOT_DECIDED: &str = "La validation n’a pas pu être enregistrée : réessayez dans un instant, \
     ou donnez-la dans l’application.";
const IN_APPLICATION: &str = "Cette action se valide dans l’application, qui en montre les \
     paramètres exacts : rien n’a été décidé.";
const AWAITS_A_MANAGER: &str = "Cette action attend la validation de votre responsable, dans \
     l’application : rien n’a été décidé.";
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
    let Some(identity) = acting_person(services, from, Act::Decide).await? else {
        return Ok(());
    };
    let mut tx = services.db.begin(identity.tenant_id).await?;
    let approval = get_approval(&mut tx, choice.approval_id).await?;
    tx.commit().await?;
    // Their own action only, nothing told of anyone else's.
    let Some(approval) = approval.filter(|a| a.requested_for_user_id == identity.user_id) else {
        say(services, from, APPROVAL_NOT_FOUND).await;
        return Ok(());
    };
    if approval.routed_to_manager() {
        say(services, from, AWAITS_A_MANAGER).await;
        return Ok(());
    }
    if !validated_in_slack(&approval.risk, &approval.payload) {
        say(services, from, IN_APPLICATION).await;
        return Ok(());
    }
    let input = DecisionInput {
        approval_id: choice.approval_id,
        approved: choice.approved,
        comment: None,
        gateway: Some(GatewayKind::Slack.as_str()),
    };
    let resumed = match services.runs.decide(identity, input).await {
        Ok(decided) => decided.resumed,
        Err(error) => {
            let Some(refusal) = error.downcast_ref::<DomainError>() else {
                return Err(error);
            };
            say(services, from, &refusal.message).await;
            return Ok(());
        }
    };
    show_outcomes(services, pressed, identity, resumed).await
}

/// Shows in the message what became of each of its actions, as they stand
/// now, in place of their buttons: the one just decided, and any other
/// decided meanwhile.
async fn show_outcomes(
    services: &Services,
    pressed: &PersonChoice,
    identity: AuthIdentity,
    resumed: bool,
) -> anyhow::Result<()> {
    let (from, choice) = (&pressed.from, &pressed.choice);
    let mut outcomes = Vec::new();
    let mut tx = services.db.begin(identity.tenant_id).await?;
    for id in buttons_in(&choice.blocks) {
        let Some(approval) = get_approval(&mut tx, id).await? else {
            continue;
        };
        if approval.requested_for_user_id != identity.user_id {
            continue;
        }
        let just_decided = id == choice.approval_id;
        if let Some(outcome) = outcome(&approval, just_decided, resumed) {
            outcomes.push((id, outcome));
        }
    }
    tx.commit().await?;
    if outcomes.is_empty() {
        return Ok(());
    }
    let blocks = decided_blocks(&choice.blocks, &outcomes);
    let text = outcomes
        .iter()
        .find(|(id, _)| *id == choice.approval_id)
        .map_or("Validation enregistrée.", |(_, text)| text.as_str());
    let slack = services.gateways.slack();
    if let Err(failure) = slack
        .update(
            &from.token,
            &choice.channel,
            &choice.message_ts,
            text,
            &blocks,
        )
        .await
    {
        let failure = failure.message();
        tracing::warn!(gateway = %from.gateway.id, %failure, "Slack message not updated");
    }
    Ok(())
}

/// What became of `approval`, in Slack's markup, once decided; `None`
/// while it waits.
fn outcome(approval: &ApprovalRow, just_decided: bool, resumed: bool) -> Option<String> {
    let said = match approval.status.as_str() {
        "approved" => "*Validée*",
        "rejected" => "*Refusée*",
        "expired" => "*Expirée* : elle n’a pas été validée à temps.",
        "cancelled" => "*Annulée*.",
        _ => return None,
    };
    let mut outcome = said.to_string();
    if matches!(approval.status.as_str(), "approved" | "rejected") {
        outcome.push_str(if just_decided {
            " par vous dans Slack."
        } else {
            "."
        });
        if just_decided && resumed {
            outcome.push_str(RESUMED);
        }
    }
    Some(outcome)
}
