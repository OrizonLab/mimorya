//! A validation given in Telegram (ADR-136): the « Valider » or « Refuser »
//! button a linked person pressed under a pause's message in their private
//! chat with the bot, sent by Telegram with the webhook's secret. Only a
//! button the bot showed under its own message there is taken: what the
//! press carries must be one of that message's buttons, as Telegram holds
//! it, whatever a client claims. Answered to Telegram at once, then decided
//! in the background as that person, once they may act (`person::may_act`),
//! with their rights as they stand, as from the application's screen. Only
//! an action awaiting their own decision, that may be validated outside the
//! application (`validated_in_gateway`), is decided there: any other is
//! left to the application. The press is answered with what became of it;
//! once decided, the buttons of the message's decided actions are taken
//! off, and the outcome said in answer to it. Audited as any decision, with
//! the service it was given from.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_core::gateways::telegram::{
    ApprovalChoice, ButtonPress, approval_choice, buttons_in, without_decided,
};
use mimorya_core::gateways::{GatewayKind, validated_in_gateway};
use mimorya_db::repositories::approval_delegations::standing_in_for;
use mimorya_db::repositories::approvals::get_approval;
use mimorya_db::repositories::gateway_identities::linked_account;
use mimorya_db::repositories::gateways::GatewayRow;
use uuid::Uuid;

use super::texts;
use super::updates::UpdateOutcome;
use crate::app::Services;
use crate::services::gateways::GatewayService;
use crate::services::gateways::concerned::{
    AWAITS_A_MANAGER, Concerned, IN_APPLICATION, NOT_DECIDED,
};
use crate::services::gateways::person::{Act, may_act};
use crate::services::runs::{APPROVAL_NOT_FOUND, DecisionInput};

/// What the press of a button the bot did not show there is answered.
const STALE: &str = "Ce bouton n’est plus valable.";
/// What a press that decided nothing is answered, its reason said apart.
const NOTHING_DECIDED: &str = "Rien n’a été décidé.";

/// A linked person's press of a validation button: to decide once Telegram
/// is answered.
pub struct TelegramChoice {
    pub tenant_id: Uuid,
    pub gateway_id: Uuid,
    pub press: ButtonPress,
    pub choice: ApprovalChoice,
    /// The bot's token, to answer with.
    pub token: String,
}

/// Never the token, whatever prints it.
impl std::fmt::Debug for TelegramChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramChoice")
            .field("gateway_id", &self.gateway_id)
            .field("approval", &self.choice.approval_id)
            .finish_non_exhaustive()
    }
}

/// What a press is answered: briefly, then in answer to its message.
struct Told {
    brief: &'static str,
    said: String,
}

impl Told {
    fn nothing(said: impl Into<String>) -> Self {
        Self {
            brief: NOTHING_DECIDED,
            said: said.into(),
        }
    }
}

impl GatewayService {
    /// What a press of a button under one of the bot's own messages, in
    /// its presser's private chat, is to the platform: a choice to decide,
    /// if the button is one the message shows and names a validation;
    /// anything else is answered as no longer valid.
    pub(super) fn take_press(
        &self,
        tenant_id: Uuid,
        gateway: &GatewayRow,
        press: ButtonPress,
        token: Option<String>,
    ) -> UpdateOutcome {
        let Some(token) = token else {
            tracing::warn!(gateway = %gateway.id, "Telegram gateway's bot token unreadable");
            return UpdateOutcome::Taken;
        };
        let bot = gateway.bot_user_id.parse::<i64>().ok();
        let own_button = bot.is_some() && press.sent_by == bot && press.shown();
        let choice = own_button.then(|| approval_choice(&press.data)).flatten();
        let Some(choice) = choice else {
            let (telegram, gateway_id) = (self.telegram.clone(), gateway.id);
            self.background.spawn(async move {
                if let Err(failure) = telegram
                    .answer_press(&token, &press.callback_id, STALE)
                    .await
                {
                    let failure = failure.message();
                    tracing::warn!(%gateway_id, %failure, "Telegram press not answered");
                }
            });
            return UpdateOutcome::Taken;
        };
        UpdateOutcome::Choice(Box::new(TelegramChoice {
            tenant_id,
            gateway_id: gateway.id,
            press,
            choice,
            token,
        }))
    }
}

/// Decides what `pressed` chose, as its person, or says why not.
pub async fn decide_in_telegram(services: &Services, pressed: TelegramChoice) {
    let told = match decide(services, &pressed).await {
        Ok(told) => told,
        Err(error) => {
            let gateway = pressed.gateway_id;
            let error = format!("{error:#}");
            tracing::warn!(%gateway, %error, "Telegram validation not decided");
            Told::nothing(NOT_DECIDED)
        }
    };
    let telegram = services.gateways.telegram();
    let press = &pressed.press;
    let token = &pressed.token;
    if let Err(failure) = telegram
        .answer_press(token, &press.callback_id, told.brief)
        .await
    {
        let failure = failure.message();
        tracing::warn!(gateway = %pressed.gateway_id, %failure, "Telegram press not answered");
    }
    // In their private chat, which their identifier names.
    let chat = press.person.id;
    if let Err(failure) = telegram
        .reply(token, chat, press.message_id, &told.said)
        .await
    {
        let failure = failure.message();
        tracing::warn!(gateway = %pressed.gateway_id, %failure, "Telegram answer not sent");
    }
}

async fn decide(services: &Services, pressed: &TelegramChoice) -> anyhow::Result<Told> {
    let (tenant_id, gateway_id) = (pressed.tenant_id, pressed.gateway_id);
    let person = pressed.press.person.id.to_string();
    let mut tx = services.db.begin(tenant_id).await?;
    let account = linked_account(&mut tx, gateway_id, &person).await?;
    tx.commit().await?;
    let Some(account) = account else {
        let unlinked = texts::unlinked(&services.gateways.assistant_name);
        return Ok(Told::nothing(unlinked));
    };
    let identity = match may_act(services, tenant_id, &account, Act::Decide).await? {
        Ok(identity) => identity,
        Err(refusal) => return Ok(Told::nothing(refusal)),
    };
    let choice = pressed.choice;
    let mut tx = services.db.begin(tenant_id).await?;
    let approval = get_approval(&mut tx, choice.approval_id).await?;
    let concerned = Concerned {
        user_id: identity.user_id,
        standing_in_for: standing_in_for(&mut tx, identity.user_id, Utc::now()).await?,
    };
    tx.commit().await?;
    // Their own action, or one awaiting them: nothing told of any other.
    let Some(approval) = approval.filter(|a| concerned.sees(a)) else {
        return Ok(Told::nothing(APPROVAL_NOT_FOUND));
    };
    if !concerned.decides(&approval) {
        return Ok(Told::nothing(AWAITS_A_MANAGER));
    }
    if !validated_in_gateway(&approval.risk, &approval.payload) {
        return Ok(Told::nothing(IN_APPLICATION));
    }
    let input = DecisionInput {
        approval_id: choice.approval_id,
        approved: choice.approved,
        comment: None,
        gateway: Some(GatewayKind::Telegram.as_str()),
    };
    let resumed = match services.runs.decide(identity, input).await {
        Ok(decided) => decided.resumed,
        Err(error) => {
            let Some(refusal) = error.downcast_ref::<DomainError>() else {
                return Err(error);
            };
            return Ok(Told::nothing(refusal.message.clone()));
        }
    };
    // Decided: a failure to take its buttons off is no reason to say
    // otherwise.
    if let Err(error) = take_off_decided(services, pressed, &concerned).await {
        let error = format!("{error:#}");
        tracing::warn!(%gateway_id, %error, "Telegram buttons not taken off");
    }
    let own = approval.requested_for_user_id == identity.user_id;
    Ok(decided(choice.approved, resumed, own))
}

/// Takes off the message the buttons of its actions decided by now: the
/// one just decided, and any other decided meanwhile; the others stay.
async fn take_off_decided(
    services: &Services,
    pressed: &TelegramChoice,
    concerned: &Concerned,
) -> anyhow::Result<()> {
    let press = &pressed.press;
    let mut decided = Vec::new();
    let mut tx = services.db.begin(pressed.tenant_id).await?;
    for id in buttons_in(&press.keyboard) {
        let approval = get_approval(&mut tx, id).await?;
        if approval.is_some_and(|a| concerned.sees(&a) && a.status != "pending") {
            decided.push(id);
        }
    }
    tx.commit().await?;
    let keyboard = without_decided(&press.keyboard, &decided);
    let telegram = services.gateways.telegram();
    let (chat, message) = (press.person.id, press.message_id);
    if let Err(failure) = telegram
        .edit_buttons(&pressed.token, chat, message, &keyboard)
        .await
    {
        let failure = failure.message();
        tracing::warn!(gateway = %pressed.gateway_id, %failure, "Telegram buttons not edited");
    }
    Ok(())
}

/// What a decision is answered: whom the agent acts for (`own`) reads that
/// their request goes on here.
fn decided(approved: bool, resumed: bool, own: bool) -> Told {
    let (brief, mut said) = if approved {
        ("Validée", "Validée par vous dans Telegram.".to_owned())
    } else {
        ("Refusée", "Refusée par vous dans Telegram.".to_owned())
    };
    if resumed {
        said.push_str(if own {
            " La demande reprend : la suite viendra ici."
        } else {
            " La demande reprend."
        });
    }
    Told { brief, said }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decision_is_answered_with_what_became_of_it() {
        let told = decided(true, true, true);
        assert_eq!(told.brief, "Validée");
        assert_eq!(
            told.said,
            "Validée par vous dans Telegram. La demande reprend : la suite viendra ici."
        );
        assert_eq!(
            decided(false, true, false).said,
            "Refusée par vous dans Telegram. La demande reprend."
        );
        assert_eq!(
            decided(true, false, true).said,
            "Validée par vous dans Telegram."
        );
        assert_eq!(Told::nothing("Non").brief, "Rien n’a été décidé.");
    }
}
