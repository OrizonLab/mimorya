//! The actions a message of the bot shows with their buttons (ADR-136,
//! ADR-137): those that may be validated outside the application
//! (`validated_in_gateway`), each with its description, its risk and its
//! exact parameters, whole, as many as one message holds; the others are
//! validated in the application, which shows them whole. Alike in the pause
//! written to whom the agent acts for and in the notice written to whom a
//! request awaits. Written again once some are settled, each as it stands
//! (ADR-148): a settled one no longer « à valider », its parameters no
//! longer those « qui seront exécutés », followed by what became of it.

use super::html::escaped;
use crate::gateways::{
    ActionState, MAX_ACTION_CHARS, ShownAction, bounded, exact_parameters, risk_label,
    validated_in_gateway,
};

/// The most actions shown with their buttons in one message.
pub const MAX_SHOWN_ACTIONS: usize = 5;
/// The most characters of shown actions in one message, which Telegram
/// takes up to 4 096 characters of text: room is left for the rest.
const MAX_SHOWN_CHARS: usize = 3_400;

/// Of `actions`, in order, those shown with their buttons in one message:
/// those that may be validated outside the application, as many as one
/// message holds.
pub fn shown_in_message<'a>(
    actions: impl IntoIterator<Item = ShownAction<'a>>,
) -> Vec<ShownAction<'a>> {
    let mut shown = Vec::new();
    for action in actions {
        if !validated_in_gateway(action.risk, action.parameters) {
            continue;
        }
        shown.push(action);
        // As Telegram counts them: the text, its markup read.
        let fits = shown_actions(&shown, false).chars().count() <= MAX_SHOWN_CHARS;
        if !fits || shown.len() > MAX_SHOWN_ACTIONS {
            shown.pop();
            break;
        }
    }
    shown
}

/// The actions shown in a message as it is sent, each awaiting a decision,
/// in Telegram's HTML (`html`) or in plain text (`stated_actions`).
pub fn shown_actions(actions: &[ShownAction], html: bool) -> String {
    stated_actions(actions.iter().map(|&a| (a, ActionState::Open)), html)
}

/// The actions shown in a message, each as it stands, in Telegram's HTML
/// (`html`) or in plain text: each numbered when there are several, its
/// description (escaped, bounded) and its risk, then its exact parameters
/// whole, as literal text; a settled one, what became of it.
pub fn stated_actions<'a>(
    actions: impl ExactSizeIterator<Item = (ShownAction<'a>, ActionState)>,
    html: bool,
) -> String {
    let several = actions.len() > 1;
    let mut text = String::new();
    for (i, (action, state)) in actions.enumerate() {
        let number = if several {
            (i + 1).to_string()
        } else {
            String::new()
        };
        let (summary, _) = bounded(action.summary.trim(), MAX_ACTION_CHARS);
        let parameters = exact_parameters(action.parameters);
        let risk = risk_label(action.risk);
        let (heading, exact) = (state.heading(&number), state.parameters_heading());
        text.push_str(&if html {
            format!(
                "\n\n<b>{heading}</b> {}\n<b>Risque :</b> {risk}. {exact}\n<pre>{}</pre>",
                escaped(&summary),
                escaped(&parameters)
            )
        } else {
            format!("\n\n{heading} {summary}\nRisque : {risk}. {exact}\n{parameters}")
        });
        if let Some(outcome) = state.outcome() {
            text.push_str(&if html {
                format!("\n<i>{outcome}</i>")
            } else {
                format!("\n{outcome}")
            });
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::*;

    fn shown<'a>(summary: &'a str, risk: &'a str, parameters: &'a Value) -> ShownAction<'a> {
        ShownAction {
            approval_id: Uuid::new_v4(),
            summary,
            risk,
            parameters,
        }
    }

    #[test]
    fn several_actions_are_numbered_and_shown_with_their_exact_parameters() {
        let parameters = json!({ "to": "<b>co</b> & cie" });
        let actions = [
            shown("Écrire à <b>", "write", &parameters),
            shown("Autre", "write", &parameters),
        ];
        let html = shown_actions(&actions, true);
        assert!(
            html.contains("<b>Action 1 à valider :</b> Écrire à &lt;b&gt;"),
            "{html}"
        );
        assert!(html.contains("<b>Risque :</b> Modification."), "{html}");
        let exact = escaped(&exact_parameters(&parameters));
        assert!(html.contains(&format!("<pre>{exact}</pre>")), "{html}");
        let plain = shown_actions(&actions, false);
        assert!(plain.contains("Action 2 à valider : Autre"), "{plain}");
        assert!(plain.contains(&exact_parameters(&parameters)), "{plain}");
        let long = "x".repeat(MAX_ACTION_CHARS * 2);
        let bounded = shown_actions(&[shown(&long, "write", &parameters)], false);
        assert!(bounded.chars().count() < MAX_ACTION_CHARS + 200);
    }

    #[test]
    fn a_settled_action_is_shown_as_such_beside_one_still_open() {
        let parameters = json!({ "intervention": "INT-2052" });
        let actions = [
            (
                shown("Planifier", "write", &parameters),
                ActionState::Approved,
            ),
            (shown("Prévenir", "write", &parameters), ActionState::Open),
            (shown("Clore", "write", &parameters), ActionState::Rejected),
            (shown("Relancer", "write", &parameters), ActionState::Closed),
        ];
        let html = stated_actions(actions.into_iter(), true);
        assert!(
            html.starts_with(
                "\n\n<b>Action 1 :</b> Planifier\n<b>Risque :</b> Modification. \
                 Paramètres exacts :\n<pre>"
            ),
            "{html}"
        );
        assert!(html.contains("</pre>\n<i>Validée.</i>\n\n<b>Action 2 à valider :</b>"));
        assert!(html.contains("<i>Refusée.</i>"), "{html}");
        assert!(
            html.ends_with("</pre>\n<i>N’attend plus de décision.</i>"),
            "{html}"
        );
        // Only the open one is still to validate, its parameters executed.
        assert_eq!(html.matches("à valider").count(), 1, "{html}");
        assert_eq!(html.matches("qui seront exécutés").count(), 1, "{html}");
        let plain = stated_actions(actions.into_iter(), false);
        assert!(plain.contains("Action 3 : Clore\n"), "{plain}");
        assert!(
            plain.contains("}\nRefusée.\n\nAction 4 : Relancer"),
            "{plain}"
        );
        // One alone: not numbered.
        let one = stated_actions([actions[0]].into_iter(), false);
        assert!(one.starts_with("\n\nAction : Planifier"), "{one}");
        assert!(one.ends_with("\nValidée."), "{one}");
    }

    #[test]
    fn only_actions_validated_outside_are_shown_as_many_as_a_message_holds() {
        let (small, to, large) = (
            json!({ "proposalId": "p-1" }),
            json!({ "to": "x" }),
            json!({ "body": "x".repeat(2_000) }),
        );
        // Sent outside the company: in the application only.
        let actions = [
            shown("A", "write", &small),
            shown("B", "external", &to),
            shown("C", "write", &small),
        ];
        let kept = shown_in_message(actions);
        let summaries: Vec<&str> = kept.iter().map(|a| a.summary).collect();
        assert_eq!(summaries, ["A", "C"]);
        // Two large ones do not fit one message: the first only.
        let actions = [
            shown("A", "write", &large),
            shown("B", "write", &large),
            shown("C", "write", &small),
        ];
        assert_eq!(shown_in_message(actions).len(), 1);
        // At most a few.
        let many = std::iter::repeat_n(shown("A", "write", &small), MAX_SHOWN_ACTIONS + 2);
        assert_eq!(shown_in_message(many).len(), MAX_SHOWN_ACTIONS);
    }
}
