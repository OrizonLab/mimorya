//! A validation given in Teams (ADR-141): the « Valider » or « Refuser »
//! button a linked person pressed on the card of a pause of their own run,
//! in their personal conversation with the company's Teams bot, sent by the
//! Bot Framework with the token Microsoft signs. Teams sends what the
//! button carries, as the client writes it, and the card it was pressed on,
//! not what the card showed: only a request the platform keeps as shown on
//! a card of its own, in that conversation, is decided (`gateway_cards`);
//! any other press is said to be no longer valid. Decided in the background
//! as that person, once they may act (`person::may_act`), with their rights
//! as they stand, as from the application's screen, by the path the other
//! services share (`concerned::decide_as`): only an action awaiting their
//! decision, that may be validated outside the application. The card is
//! then written again without the buttons of its actions decided by then
//! (now, or meanwhile in the application), unless it shows an action
//! its presser does not see; what became of the press is said in the
//! conversation. Audited as any decision, with the service, `teams`.

use mimorya_core::gateways::teams::{
    CardAction, CardChoice, PersonalMessage, TeamsApplication, escaped, validation_card,
};
use mimorya_core::gateways::{GatewayKind, ShownAction};
use mimorya_db::repositories::approvals::{ApprovalRow, get_approval};
use mimorya_db::repositories::gateway_cards::{GatewayCard, find_gateway_card};
use mimorya_db::repositories::gateway_identities::linked_account;
use uuid::Uuid;

use super::reply_text::{Pause, reply_text};
use super::texts;
use crate::app::Services;
use crate::services::gateways::concerned::{Concerned, NOT_DECIDED, decide_as};
use crate::services::gateways::person::{Act, may_act};

/// What the press of a button the bot did not show there is told.
const STALE: &str = "Ce bouton n’est plus valable : rien n’a été décidé.";

/// A linked person's press of a card's button, to decide.
pub struct TeamsChoice {
    pub tenant_id: Uuid,
    pub gateway_id: Uuid,
    /// The bot's application, to answer with.
    pub app: TeamsApplication,
    pub message: PersonalMessage,
    pub choice: CardChoice,
}

/// Never the client secret, whatever prints it.
impl std::fmt::Debug for TeamsChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeamsChoice")
            .field("gateway_id", &self.gateway_id)
            .field("approval", &self.choice.approval_id)
            .finish_non_exhaustive()
    }
}

/// An action awaiting validation, as a card shows it.
pub(in crate::services::gateways) fn shown_action(approval: &ApprovalRow) -> ShownAction<'_> {
    ShownAction {
        approval_id: approval.id,
        summary: &approval.action_summary,
        risk: &approval.risk,
        parameters: &approval.payload,
    }
}

/// Decides what `pressed` chose, as its person, or says why not.
pub async fn decide_in_teams(services: &Services, pressed: TeamsChoice) {
    let said = match decide(services, &pressed).await {
        Ok(said) => said,
        Err(error) => {
            let gateway = pressed.gateway_id;
            let error = format!("{error:#}");
            tracing::warn!(%gateway, %error, "Teams validation not decided");
            NOT_DECIDED.to_owned()
        }
    };
    let message = &pressed.message;
    let to = (
        message.service_url.as_str(),
        message.conversation_id.as_str(),
    );
    let teams = services.gateways.teams();
    if let Err(failure) = teams.send(&pressed.app, to, &escaped(&said)).await {
        let failure = failure.message();
        tracing::warn!(gateway = %pressed.gateway_id, %failure, "Teams answer not sent");
    }
}

async fn decide(services: &Services, pressed: &TeamsChoice) -> anyhow::Result<String> {
    let (tenant_id, gateway_id) = (pressed.tenant_id, pressed.gateway_id);
    let message = &pressed.message;
    let mut tx = services.db.begin(tenant_id).await?;
    let account = linked_account(&mut tx, gateway_id, &message.person.aad_object_id).await?;
    let card = match &message.reply_to_id {
        Some(card) => {
            find_gateway_card(&mut tx, gateway_id, &message.conversation_id, card).await?
        }
        None => None,
    };
    tx.commit().await?;
    let Some(account) = account else {
        return Ok(texts::UNLINKED.to_owned());
    };
    let identity = match may_act(services, tenant_id, &account, Act::Decide).await? {
        Ok(identity) => identity,
        Err(refusal) => return Ok(refusal),
    };
    // A button of a card of the bot's own, there, showing that request.
    let approval_id = pressed.choice.approval_id;
    let Some(card) = card.filter(|card| card.approval_ids.contains(&approval_id)) else {
        return Ok(STALE.to_owned());
    };
    let choice = (approval_id, pressed.choice.approved);
    let (concerned, decided) = decide_as(services, identity, choice, GatewayKind::Teams).await?;
    // Decided now, or meanwhile: a failure to write the card again is no
    // reason to say otherwise.
    if let Err(error) = rewrite_card(services, pressed, &card, &concerned).await {
        let error = format!("{error:#}");
        tracing::warn!(%gateway_id, %error, "Teams card not written again");
    }
    Ok(match decided {
        Ok(decided) => outcome(pressed.choice.approved, decided.resumed, decided.own),
        Err(refusal) => refusal,
    })
}

/// Writes the card again as it was, without the buttons of its actions
/// decided by now. Left as it is when none is, or when it shows an action
/// its presser does not see (a Teams account linked since to someone else
/// learns nothing of the actions of the person before).
async fn rewrite_card(
    services: &Services,
    pressed: &TeamsChoice,
    card: &GatewayCard,
    concerned: &Concerned,
) -> anyhow::Result<()> {
    let mut approvals = Vec::new();
    let mut tx = services.db.begin(pressed.tenant_id).await?;
    for id in &card.approval_ids {
        match get_approval(&mut tx, *id).await? {
            Some(approval) if concerned.sees(&approval) => approvals.push(approval),
            _ => return Ok(()),
        }
    }
    tx.commit().await?;
    if approvals.iter().all(|a| a.status == "pending") {
        return Ok(());
    }
    let actions: Vec<CardAction> = (approvals.iter())
        .map(|a| CardAction {
            shown: shown_action(a),
            open: a.status == "pending",
        })
        .collect();
    let pause = Pause {
        shown: actions.len(),
        in_application: usize::try_from(card.in_application).unwrap_or(0),
        managed: usize::try_from(card.managed).unwrap_or(0),
    };
    let url = services.gateways.conversation_url(card.conversation_id);
    let text = reply_text("awaiting_approval", None, &url, pause);
    let message = &pressed.message;
    let to = (
        message.service_url.as_str(),
        message.conversation_id.as_str(),
    );
    let written = (text.as_str(), &validation_card(&actions));
    // Pressed on it: the card the message answers.
    let activity = message.reply_to_id.as_deref().unwrap_or_default();
    let teams = services.gateways.teams();
    if let Err(failure) = teams.update_card(&pressed.app, to, activity, written).await {
        anyhow::bail!("Teams refused the card: {}", failure.message());
    }
    Ok(())
}

/// What a decision is told: whom the agent acts for (`own`) reads that
/// their request goes on here.
fn outcome(approved: bool, resumed: bool, own: bool) -> String {
    let mut said = if approved {
        "Validée par vous dans Teams.".to_owned()
    } else {
        "Refusée par vous dans Teams.".to_owned()
    };
    if resumed {
        said.push_str(if own {
            " La demande reprend : la suite viendra ici."
        } else {
            " La demande reprend."
        });
    }
    said
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decision_is_told_with_what_became_of_it() {
        assert_eq!(
            outcome(true, true, true),
            "Validée par vous dans Teams. La demande reprend : la suite viendra ici."
        );
        assert_eq!(
            outcome(false, true, false),
            "Refusée par vous dans Teams. La demande reprend."
        );
        assert_eq!(outcome(true, false, true), "Validée par vous dans Teams.");
    }
}
