//! What a message of the bot showing actions with their buttons is written
//! again with once some of them are settled (ADR-148), in the HTML Telegram
//! reads or in plain text: each action as it stands (`stated_actions`), a
//! settled one no longer « à valider » and followed by what became of it;
//! while one awaits a decision, the words the message was written with;
//! once none does, that none awaits a decision any more, a pause promising
//! its answer only once each was validated or refused, as in Teams
//! (ADR-141, ADR-142).

use mimorya_core::gateways::ActionState;
use mimorya_core::gateways::telegram::stated_actions;

use super::approver_text::{notice_words, requested_for, validations_link};
use super::reply_text::{Markup, Pause, awaiting_words, others};
use crate::services::gateways::approver_text::ApproverNotice;
use crate::services::gateways::settled_words::{settled_notice, settled_pause};

/// The pause `pause`, whose shown actions stand as `states`, written again
/// in `markup`, its conversation at `url`.
pub fn pause_again(markup: Markup, url: &str, pause: Pause, states: &[ActionState]) -> String {
    let link = markup.conversation(url);
    let mut text = if states.contains(&ActionState::Open) {
        awaiting_words(&link, pause)
    } else {
        pause_settled(&link, pause, states)
    };
    let stated = pause.shown.iter().copied().zip(states.iter().copied());
    text.push_str(&stated_actions(stated, markup == Markup::Html));
    text
}

/// The notice `notice`, whose shown actions stand as `states`, written
/// again in `markup`.
pub fn notice_again(markup: Markup, notice: &ApproverNotice, states: &[ActionState]) -> String {
    let mut text = if states.contains(&ActionState::Open) {
        notice_words(markup, notice)
    } else {
        notice_settled(markup, notice, states)
    };
    let stated = notice.shown.iter().copied().zip(states.iter().copied());
    text.push_str(&stated_actions(stated, markup == Markup::Html));
    text
}

/// What a pause none of whose shown actions awaits a decision any more says
/// before them: decided, each validated or refused, its answer to come
/// here, with its others; or no longer awaiting a decision.
fn pause_settled(link: &str, pause: Pause, states: &[ActionState]) -> String {
    let decided = states.iter().all(|state| state.is_decided());
    let settled = settled_pause(decided, pause.shown.len() == 1);
    if !decided {
        return format!("{settled} ({link}).");
    }
    format!("{settled} ({link}). La suite viendra ici.{}", others(pause))
}

/// What a notice none of whose shown actions awaits a decision any more
/// says before them, and where its others are.
fn notice_settled(markup: Markup, notice: &ApproverNotice, states: &[ActionState]) -> String {
    let decided = states.iter().all(|state| state.is_decided());
    let settled = settled_notice(decided, notice.shown.len() == 1);
    let who = requested_for(markup, notice.requested_for);
    let mut text = format!("{who} : {settled}.");
    if notice.in_application > 0 {
        let link = validations_link(markup, notice.link);
        text.push_str(&format!(
            " D’autres sont à valider dans l’application ({link})."
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use mimorya_core::gateways::ShownAction;
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::*;
    use ActionState::{Approved, Closed, Open, Rejected};

    const URL: &str = "https://app.example/chat/42";
    const LINK: &str = "<a href=\"https://app.example/chat/42\">ouvrir la conversation</a>";

    fn shown<'a>(summary: &'a str, parameters: &'a Value) -> ShownAction<'a> {
        ShownAction {
            approval_id: Uuid::new_v4(),
            summary,
            risk: "write",
            parameters,
        }
    }

    #[test]
    fn a_pause_still_awaiting_one_action_shows_the_other_as_settled() {
        let parameters = json!({ "intervention": "INT-2052" });
        let actions = [
            shown("Planifier", &parameters),
            shown("Prévenir", &parameters),
        ];
        let pause = Pause {
            shown: &actions,
            in_application: 1,
            managed: 0,
        };
        let text = pause_again(Markup::Html, URL, pause, &[Approved, Open]);
        assert!(
            text.starts_with("Des actions de cette demande attendent votre validation"),
            "{text}"
        );
        assert!(text.contains("<b>Action 1 :</b> Planifier"), "{text}");
        assert!(text.contains("<i>Validée.</i>"), "{text}");
        assert!(
            text.contains("<b>Action 2 à valider :</b> Prévenir"),
            "{text}"
        );
        assert!(
            text.contains("D’autres sont à valider dans l’application."),
            "{text}"
        );
    }

    #[test]
    fn a_pause_none_of_whose_actions_awaits_promises_its_answer_once_decided() {
        let parameters = json!({});
        let actions = [
            shown("Planifier", &parameters),
            shown("Prévenir", &parameters),
        ];
        let pause = Pause {
            shown: &actions,
            in_application: 0,
            managed: 1,
        };
        let decided = pause_again(Markup::Html, URL, pause, &[Approved, Rejected]);
        assert!(
            decided.starts_with(&format!(
                "Les actions de cette demande montrées ci-dessous sont décidées ({LINK}). La \
                 suite viendra ici. D’autres attendent la validation de votre responsable.\n\n"
            )),
            "{decided}"
        );
        assert!(!decided.contains("à valider"), "{decided}");
        assert!(!decided.contains("qui seront exécutés"), "{decided}");
        let closed = pause_again(Markup::Plain, URL, pause, &[Approved, Closed]);
        assert!(
            closed.starts_with(&format!(
                "Les actions de cette demande montrées ci-dessous n’attendent plus de décision \
                 ({URL}).\n\n"
            )),
            "{closed}"
        );
        assert!(closed.ends_with("N’attend plus de décision."), "{closed}");
        let one = pause_again(
            Markup::Plain,
            URL,
            Pause {
                shown: &actions[..1],
                ..pause
            },
            &[Rejected],
        );
        assert!(
            one.starts_with("L’action de cette demande montrée ci-dessous est décidée"),
            "{one}"
        );
        assert!(one.contains("\n\nAction : Planifier\n"), "{one}");
        assert!(one.ends_with("\nRefusée."), "{one}");
    }

    #[test]
    fn a_notice_says_its_actions_are_settled_and_where_the_others_are() {
        const VALIDATIONS: &str = "https://app.example/approvals";
        let parameters = json!({});
        let actions = [shown("Planifier", &parameters)];
        let mut notice = ApproverNotice {
            requested_for: "Nadia <Benali>",
            absent: Some("Marc"),
            shown: &actions,
            in_application: 1,
            link: VALIDATIONS,
        };
        let open = notice_again(Markup::Html, &notice, &[Open]);
        assert!(
            open.starts_with("<b>Nadia &lt;Benali&gt;</b> : l’agent attend votre validation"),
            "{open}"
        );
        assert!(
            open.contains("<b>Action à valider :</b> Planifier"),
            "{open}"
        );
        let settled = notice_again(Markup::Html, &notice, &[Approved]);
        assert!(
            settled.starts_with(
                "<b>Nadia &lt;Benali&gt;</b> : l’action montrée ci-dessous est décidée. \
                 D’autres sont à valider dans l’application (<a \
                 href=\"https://app.example/approvals\">ouvrir les validations</a>).\n\n\
                 <b>Action :</b> Planifier"
            ),
            "{settled}"
        );
        assert!(settled.ends_with("<i>Validée.</i>"), "{settled}");
        notice.in_application = 0;
        let closed = notice_again(Markup::Plain, &notice, &[Closed]);
        assert!(
            closed.starts_with(
                "Nadia <Benali> : l’action montrée ci-dessous n’attend plus de décision.\n\n"
            ),
            "{closed}"
        );
    }
}
