//! The actions a message of the bot shows with their buttons (ADR-136,
//! ADR-137): those that may be validated outside the application
//! (`validated_in_gateway`), each with its description, its risk and its
//! exact parameters, whole, as many as one message holds; the others are
//! validated in the application, which shows them whole. Alike in the pause
//! written to whom the agent acts for and in the notice written to whom a
//! request awaits.

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

/// The actions shown in a message, in Telegram's HTML (`html`) or in plain
/// text: each numbered when there are several, its description (escaped,
/// bounded) and its risk, then its exact parameters whole, as literal text.
pub fn shown_actions(actions: &[ShownAction], html: bool) -> String {
    let mut text = String::new();
    for (i, action) in actions.iter().enumerate() {
        let number = if actions.len() > 1 {
            (i + 1).to_string()
        } else {
            String::new()
        };
        let (summary, _) = bounded(action.summary.trim(), MAX_ACTION_CHARS);
        let parameters = exact_parameters(action.parameters);
        let risk = risk_label(action.risk);
        // Shown with their buttons: open.
        let (heading, exact) = (
            ActionState::Open.heading(&number),
            ActionState::Open.parameters_heading(),
        );
        text.push_str(&if html {
            format!(
                "\n\n<b>{heading}</b> {}\n<b>Risque :</b> {risk}. {exact}\n<pre>{}</pre>",
                escaped(&summary),
                escaped(&parameters)
            )
        } else {
            format!("\n\n{heading} {summary}\nRisque : {risk}. {exact}\n{parameters}")
        });
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
