//! The validations given in Telegram (ADR-136): a run started there that
//! pauses for its author's own validation shows, in their private chat with
//! the bot, each action awaiting it that may be validated there
//! (`validated_in_gateway`) with its exact parameters, whole, and a
//! « Valider » and a « Refuser » button under the message. What a button
//! carries names its approval and the choice (`v:` or `r:` and the
//! approval's identifier, within Telegram's 64 bytes). Once an action is
//! decided, its buttons are taken off the message; the others stay.

use serde_json::{Value, json};
use uuid::Uuid;

use super::html::escaped;
use super::press::KeyboardButton;
use crate::gateways::{MAX_ACTION_CHARS, ShownAction, bounded, exact_parameters, risk_label};

/// What a « Valider » button carries before its approval, and a « Refuser »
/// one.
const APPROVE_PREFIX: &str = "v:";
const REJECT_PREFIX: &str = "r:";

/// A person's choice on an action awaiting their validation, as the button
/// they pressed carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprovalChoice {
    pub approval_id: Uuid,
    pub approved: bool,
}

/// The choice what a button carries tells, if it is one of a validation's.
pub fn approval_choice(data: &str) -> Option<ApprovalChoice> {
    let (approved, id) = match (
        data.strip_prefix(APPROVE_PREFIX),
        data.strip_prefix(REJECT_PREFIX),
    ) {
        (Some(id), _) => (true, id),
        (_, Some(id)) => (false, id),
        _ => return None,
    };
    let approval_id = Uuid::parse_str(id).ok()?;
    // As the platform writes them, nothing else.
    (approval_id.to_string() == id).then_some(ApprovalChoice {
        approval_id,
        approved,
    })
}

/// The buttons under a pause's message: for each action, a row with its
/// « Valider » and « Refuser » buttons, numbered when there are several.
pub fn approval_keyboard(actions: &[ShownAction]) -> Value {
    let rows: Vec<Value> = (actions.iter().enumerate())
        .map(|(i, action)| {
            let which = if actions.len() > 1 {
                format!(" l’action {}", i + 1)
            } else {
                String::new()
            };
            let id = action.approval_id;
            json!([
                { "text": format!("Valider{which}"), "callback_data": format!("{APPROVE_PREFIX}{id}") },
                { "text": format!("Refuser{which}"), "callback_data": format!("{REJECT_PREFIX}{id}") },
            ])
        })
        .collect();
    json!({ "inline_keyboard": rows })
}

/// The actions shown in a pause's message, in Telegram's HTML (`html`) or
/// in plain text: each numbered when there are several, its description
/// (escaped, bounded) and its risk, then its exact parameters whole, as
/// literal text.
pub fn shown_actions(actions: &[ShownAction], html: bool) -> String {
    let mut text = String::new();
    for (i, action) in actions.iter().enumerate() {
        let number = if actions.len() > 1 {
            format!(" {}", i + 1)
        } else {
            String::new()
        };
        let (summary, _) = bounded(action.summary.trim(), MAX_ACTION_CHARS);
        let parameters = exact_parameters(action.parameters);
        let risk = risk_label(action.risk);
        text.push_str(&if html {
            format!(
                "\n\n<b>Action{number} à valider :</b> {}\n<b>Risque :</b> {risk}. Paramètres \
                 exacts qui seront exécutés :\n<pre>{}</pre>",
                escaped(&summary),
                escaped(&parameters)
            )
        } else {
            format!(
                "\n\nAction{number} à valider : {summary}\nRisque : {risk}. Paramètres exacts \
                 qui seront exécutés :\n{parameters}"
            )
        });
    }
    text
}

/// The approvals whose buttons are in `keyboard`.
pub fn buttons_in(keyboard: &[Vec<KeyboardButton>]) -> Vec<Uuid> {
    let mut ids: Vec<Uuid> = (keyboard.iter().flatten())
        .filter_map(|button| approval_choice(&button.data))
        .map(|choice| choice.approval_id)
        .collect();
    ids.dedup();
    ids
}

/// `keyboard` without the buttons of the approvals `decided`: the others
/// as they were.
pub fn without_decided(keyboard: &[Vec<KeyboardButton>], decided: &[Uuid]) -> Value {
    let rows: Vec<Value> = (keyboard.iter())
        .filter_map(|row| {
            let kept: Vec<Value> = (row.iter())
                .filter(|button| {
                    approval_choice(&button.data)
                        .is_none_or(|choice| !decided.contains(&choice.approval_id))
                })
                .map(|button| json!({ "text": button.text, "callback_data": button.data }))
                .collect();
            (!kept.is_empty()).then(|| json!(kept))
        })
        .collect();
    json!({ "inline_keyboard": rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPROVAL: &str = "7b0e4c4e-55d0-4f55-9a43-2d1c1c7d2f10";

    fn id() -> Uuid {
        Uuid::parse_str(APPROVAL).unwrap()
    }

    fn shown<'a>(id: Uuid, summary: &'a str, parameters: &'a Value) -> ShownAction<'a> {
        ShownAction {
            approval_id: id,
            summary,
            risk: "write",
            parameters,
        }
    }

    fn read(keyboard: &Value) -> Vec<Vec<KeyboardButton>> {
        (keyboard["inline_keyboard"].as_array().unwrap().iter())
            .map(|row| {
                (row.as_array().unwrap().iter())
                    .map(|b| KeyboardButton {
                        text: b["text"].as_str().unwrap().into(),
                        data: b["callback_data"].as_str().unwrap().into(),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_button_names_its_approval_and_the_choice_within_telegram_s_bounds() {
        let parameters = json!({});
        let keyboard = approval_keyboard(&[shown(id(), "Appliquer", &parameters)]);
        let row = &keyboard["inline_keyboard"][0];
        assert_eq!(row[0]["text"], "Valider");
        assert_eq!(row[1]["text"], "Refuser");
        let approve = row[0]["callback_data"].as_str().unwrap();
        assert!(approve.len() <= 64);
        assert_eq!(
            approval_choice(approve),
            Some(ApprovalChoice {
                approval_id: id(),
                approved: true
            })
        );
        let reject = row[1]["callback_data"].as_str().unwrap();
        assert_eq!(approval_choice(reject).map(|c| c.approved), Some(false));
        for data in [
            "",
            "v:",
            "x:7b0e4c4e-55d0-4f55-9a43-2d1c1c7d2f10",
            "v:pas-un-id",
            "v:7B0E4C4E-55D0-4F55-9A43-2D1C1C7D2F10",
            "v:7b0e4c4e55d04f559a432d1c1c7d2f10",
        ] {
            assert_eq!(approval_choice(data), None, "{data}");
        }
    }

    #[test]
    fn several_actions_are_numbered_and_shown_with_their_exact_parameters() {
        let parameters = json!({ "to": "<b>co</b> & cie" });
        let other = Uuid::new_v4();
        let actions = [
            shown(id(), "Écrire à <b>", &parameters),
            shown(other, "Autre", &parameters),
        ];
        let keyboard = approval_keyboard(&actions);
        assert_eq!(
            keyboard["inline_keyboard"][1][0]["text"],
            "Valider l’action 2"
        );
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
        let bounded = shown_actions(&[shown(id(), &long, &parameters)], false);
        assert!(bounded.chars().count() < MAX_ACTION_CHARS + 200);
    }

    #[test]
    fn a_decided_action_s_buttons_are_taken_off_the_others_kept() {
        let parameters = json!({});
        let other = Uuid::new_v4();
        let keyboard = read(&approval_keyboard(&[
            shown(id(), "A", &parameters),
            shown(other, "B", &parameters),
        ]));
        assert_eq!(buttons_in(&keyboard), [id(), other]);
        let left = without_decided(&keyboard, &[id()]);
        let rows = left["inline_keyboard"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0]["text"], "Valider l’action 2");
        let none = without_decided(&keyboard, &[id(), other]);
        assert_eq!(none, json!({ "inline_keyboard": [] }));
    }
}
