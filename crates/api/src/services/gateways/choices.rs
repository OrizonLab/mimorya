//! A validation given in Slack (ADR-129): the « Valider » or « Refuser »
//! button a linked person pressed under an action awaiting their
//! validation, sent by Slack, signed, to the gateway's own address
//! (form-encoded, where events are JSON): an action of theirs, in its
//! thread, or one a manager decides, in the direct message telling that
//! manager, or whom they named to stand in for them, of it (ADR-130).
//! Answered to Slack at once, then decided in the background as that
//! person, once they may act (`person`), with their rights as they stand,
//! as from the application's screen. Only an action awaiting their own
//! decision, that Slack may validate (`validated_in_gateway`: changing
//! nothing outside the company, undone, its exact parameters shown whole)
//! is decided there: whatever the press says, any other is left to the
//! application. The message then shows what became of each of its actions
//! in place of their buttons; why nothing was decided is said in its
//! thread. Audited as any decision, with the service it was given from.

use mimorya_core::gateways::slack::{
    ApprovalChoice, DirectMessage, approval_choice, buttons_in, decided_blocks,
};
use mimorya_core::gateways::{ActionState, GatewayKind};
use mimorya_db::repositories::approvals::{ApprovalRow, get_approval};
use mimorya_db::repositories::gateways::GatewayRow;
use uuid::Uuid;

use super::concerned::{Concerned, NOT_DECIDED, decide_as};
use super::person::{Act, acting_person, say};
use super::{EventOutcome, GatewayService, PersonMessage, bot_token_name};
use crate::app::Services;

/// Said to whom the agent acts for, in their request's thread.
const RESUMED: &str = " La demande reprend : la suite viendra dans ce fil.";
/// Said to a manager, or whom they named to stand in for them.
const RESUMED_FOR_THEM: &str = " La demande reprend.";

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
    let pressed_choice = (choice.approval_id, choice.approved);
    let (concerned, decided) =
        decide_as(services, identity, pressed_choice, GatewayKind::Slack).await?;
    let decided = match decided {
        Ok(decided) => decided,
        Err(refusal) => {
            say(services, from, &refusal).await;
            return Ok(());
        }
    };
    // Decided: a failure to show it is no reason to say otherwise.
    if let Err(error) = show_outcomes(services, pressed, &concerned, decided.resumed).await {
        let gateway = from.gateway.id;
        tracing::warn!(%gateway, error = format!("{error:#}"), "Slack outcome not shown");
    }
    Ok(())
}

/// Shows in the message what became of each of its actions, as they stand
/// now, in place of their buttons: the one just decided, and any other
/// decided meanwhile.
async fn show_outcomes(
    services: &Services,
    pressed: &PersonChoice,
    concerned: &Concerned,
    resumed: bool,
) -> anyhow::Result<()> {
    let (from, choice) = (&pressed.from, &pressed.choice);
    let mut outcomes = Vec::new();
    let mut tx = services.db.begin(from.tenant_id).await?;
    for id in buttons_in(&choice.blocks) {
        let Some(approval) = get_approval(&mut tx, id).await? else {
            continue;
        };
        if !concerned.sees(&approval) {
            continue;
        }
        let just_decided = id == choice.approval_id;
        let own = approval.requested_for_user_id == concerned.user_id;
        if let Some(outcome) = outcome(&approval, just_decided, resumed, own) {
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

/// What became of `approval`, in Slack's markup, once settled
/// (`ActionState`); `None` while it waits. Said more closely than in the
/// other services: who decided it, or why it no longer awaits a decision.
/// Whom the agent acts for (`own`) reads that their request goes on in
/// this thread.
fn outcome(approval: &ApprovalRow, just_decided: bool, resumed: bool, own: bool) -> Option<String> {
    let state = ActionState::of(&approval.status);
    let said = match (state, approval.status.as_str()) {
        (ActionState::Open, _) => return None,
        (ActionState::Approved, _) => "*Validée*",
        (ActionState::Rejected, _) => "*Refusée*",
        (ActionState::Closed, "expired") => "*Expirée* : elle n’a pas été validée à temps.",
        (ActionState::Closed, "cancelled") => "*Annulée*.",
        (ActionState::Closed, _) => "*N’attend plus de décision*.",
    };
    let mut outcome = said.to_string();
    if state.is_decided() {
        outcome.push_str(if just_decided {
            " par vous dans Slack."
        } else {
            "."
        });
        if just_decided && resumed {
            outcome.push_str(if own { RESUMED } else { RESUMED_FOR_THEM });
        }
    }
    Some(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn with_status(status: &str) -> ApprovalRow {
        ApprovalRow {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            requested_for_user_id: Uuid::new_v4(),
            tool_name: "planning_apply".into(),
            tool_call_id: "call-1".into(),
            action_summary: "Appliquer".into(),
            payload: json!({}),
            risk: "write".into(),
            reason: String::new(),
            status: status.into(),
            decided_by_user_id: None,
            decided_at: None,
            decision_comment: None,
            expires_at: None,
            created_at: Utc::now(),
            routing: "requester".into(),
            approver_user_id: None,
        }
    }

    #[test]
    fn each_action_shows_what_became_of_it() {
        let approved = with_status("approved");
        assert_eq!(
            outcome(&approved, true, true, true).as_deref(),
            Some(
                "*Validée* par vous dans Slack. La demande reprend : la suite viendra dans ce fil."
            )
        );
        // Told to a manager, the request going on elsewhere.
        assert_eq!(
            outcome(&approved, true, true, false).as_deref(),
            Some("*Validée* par vous dans Slack. La demande reprend.")
        );
        assert_eq!(
            outcome(&approved, true, false, true).as_deref(),
            Some("*Validée* par vous dans Slack.")
        );
        // Another action of the message, decided meanwhile.
        assert_eq!(
            outcome(&approved, false, true, true).as_deref(),
            Some("*Validée*.")
        );
        let rejected = with_status("rejected");
        assert_eq!(
            outcome(&rejected, true, false, false).as_deref(),
            Some("*Refusée* par vous dans Slack.")
        );
        assert_eq!(
            outcome(&rejected, false, false, true).as_deref(),
            Some("*Refusée*.")
        );
        assert_eq!(
            outcome(&with_status("expired"), false, false, true).as_deref(),
            Some("*Expirée* : elle n’a pas été validée à temps.")
        );
        assert_eq!(
            outcome(&with_status("cancelled"), false, false, true).as_deref(),
            Some("*Annulée*.")
        );
        assert_eq!(outcome(&with_status("pending"), false, false, true), None);
        // Any other status: settled all the same, as in the other services.
        assert_eq!(
            outcome(&with_status("withdrawn"), false, false, true).as_deref(),
            Some("*N’attend plus de décision*.")
        );
    }
}
