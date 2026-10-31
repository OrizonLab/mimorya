//! A validation given in Telegram: the « Valider » or « Refuser » button a
//! linked person pressed under a message of the bot in their private chat
//! with it, a pause of their own run (ADR-136) or the notice of requests
//! awaiting them as a manager, or as whom stands in for one (ADR-137), sent
//! by Telegram with the webhook's secret. Only a button the bot showed
//! under its own message there is taken: what the press carries must be
//! one of that message's buttons, as Telegram holds it, whatever a client
//! claims. Answered to Telegram at once, then decided in the background as
//! that person, once they may act (`person::may_act`), with their rights as
//! they stand, as from the application's screen. Only an action awaiting
//! their decision (their own, or a manager's they are or stand in for),
//! that may be validated outside the application (`validated_in_gateway`),
//! is decided there: any other is left to the application. The press is
//! answered with what became of it; the message is written again, its
//! actions decided by then (now, or meanwhile in the application) without
//! their buttons and said as settled (`rewrite`, ADR-148), and the outcome
//! said in answer to it. Audited as any decision, with the service it was
//! given from, as in Slack (`concerned::decide_as`).

use mimorya_core::gateways::GatewayKind;
use mimorya_core::gateways::telegram::{ApprovalChoice, ButtonPress, approval_choice};
use mimorya_db::repositories::gateway_identities::linked_account;
use mimorya_db::repositories::gateways::GatewayRow;
use uuid::Uuid;

use super::rewrite::write_again;
use super::texts;
use super::updates::UpdateOutcome;
use crate::app::Services;
use crate::services::gateways::GatewayService;
use crate::services::gateways::concerned::{Concerned, NOT_DECIDED, decide_as};
use crate::services::gateways::person::{Act, may_act};

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

/// Decides what `pressed` chose, as its person, or says why not: the press
/// answered first, then its message written again, then what became of it
/// said in answer to the message.
pub async fn decide_in_telegram(services: &Services, pressed: TelegramChoice) {
    let gateway_id = pressed.gateway_id;
    let (told, concerned) = match decide(services, &pressed).await {
        Ok(decided) => decided,
        Err(error) => {
            let error = format!("{error:#}");
            tracing::warn!(%gateway_id, %error, "Telegram validation not decided");
            (Told::nothing(NOT_DECIDED), None)
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
        tracing::warn!(%gateway_id, %failure, "Telegram press not answered");
    }
    // Decided now, or meanwhile (in the application, or expired): its
    // message is written again, a failure to do so being no reason to say
    // otherwise.
    if let Some(concerned) = concerned
        && let Err(error) = write_again(services, &pressed, &concerned).await
    {
        let error = format!("{error:#}");
        tracing::warn!(%gateway_id, %error, "Telegram message not written again");
    }
    // In their private chat, which their identifier names.
    let chat = press.person.id;
    if let Err(failure) = telegram
        .reply(token, chat, press.message_id, &told.said)
        .await
    {
        let failure = failure.message();
        tracing::warn!(%gateway_id, %failure, "Telegram answer not sent");
    }
}

/// What `pressed` is answered, and who pressed it once they may decide:
/// whose message is then written again.
async fn decide(
    services: &Services,
    pressed: &TelegramChoice,
) -> anyhow::Result<(Told, Option<Concerned>)> {
    let (tenant_id, gateway_id) = (pressed.tenant_id, pressed.gateway_id);
    let person = pressed.press.person.id.to_string();
    let mut tx = services.db.begin(tenant_id).await?;
    let account = linked_account(&mut tx, gateway_id, &person).await?;
    tx.commit().await?;
    let Some(account) = account else {
        let unlinked = texts::unlinked(&services.gateways.assistant_name);
        return Ok((Told::nothing(unlinked), None));
    };
    let identity = match may_act(services, tenant_id, &account, Act::Decide).await? {
        Ok(identity) => identity,
        Err(refusal) => return Ok((Told::nothing(refusal), None)),
    };
    let choice = (pressed.choice.approval_id, pressed.choice.approved);
    let (concerned, decided) = decide_as(services, identity, choice, GatewayKind::Telegram).await?;
    let told = match decided {
        Ok(decided) => outcome(pressed.choice.approved, decided.resumed, decided.own),
        Err(refusal) => Told::nothing(refusal),
    };
    Ok((told, Some(concerned)))
}

/// What a decision is answered: whom the agent acts for (`own`) reads that
/// their request goes on here.
fn outcome(approved: bool, resumed: bool, own: bool) -> Told {
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
        let told = outcome(true, true, true);
        assert_eq!(told.brief, "Validée");
        assert_eq!(
            told.said,
            "Validée par vous dans Telegram. La demande reprend : la suite viendra ici."
        );
        assert_eq!(
            outcome(false, true, false).said,
            "Refusée par vous dans Telegram. La demande reprend."
        );
        assert_eq!(
            outcome(true, false, true).said,
            "Validée par vous dans Telegram."
        );
        assert_eq!(Told::nothing("Non").brief, "Rien n’a été décidé.");
    }
}
