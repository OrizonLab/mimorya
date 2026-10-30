//! The card the Teams bot shows a pause's actions on (ADR-141): those that
//! may be validated outside the application (`validated_in_gateway`), as
//! many as one card holds, each with its description, its risk and its
//! exact parameters, whole, then its « Valider » and « Refuser » buttons;
//! once decided, or awaiting nothing more (expired, its request stopped),
//! an action stays shown, without buttons, saying so. Every text the
//! card shows is a run of plain text (`TextRun`), which Teams never reads
//! as Markdown: nothing the agent wrote reads as markup there. A button
//! carries its choice and its approval; pressed, Teams sends it back as a
//! message whose value it is (`card_choice`), with the card it was pressed
//! on, which the platform keeps.

use serde_json::{Value, json};
use uuid::Uuid;

use crate::gateways::{
    MAX_ACTION_CHARS, ShownAction, bounded, exact_parameters, risk_label, validated_in_gateway,
};

/// The most actions shown on one card.
pub const MAX_CARD_ACTIONS: usize = 5;
/// The most characters of shown actions on one card, as in Telegram: Teams
/// takes about 28 kibibytes a message, card included.
const MAX_CARD_CHARS: usize = 3_400;
/// The key of what a button carries, among what Teams adds to it.
const KEY: &str = "mimorya";

/// An action on a card, and where it stands.
#[derive(Debug, Clone, Copy)]
pub struct CardAction<'a> {
    pub shown: ShownAction<'a>,
    pub state: CardState,
}

/// Where an action on a card stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    /// Awaiting a decision: its buttons shown.
    Open,
    /// Validated or refused.
    Decided,
    /// Awaiting nothing more, undecided: expired, or its request stopped.
    Closed,
}

impl CardState {
    /// Where an action whose request's status is `status` stands.
    pub fn of(status: &str) -> Self {
        match status {
            "pending" => Self::Open,
            "approved" | "rejected" => Self::Decided,
            _ => Self::Closed,
        }
    }
}

/// A button of a card, pressed: the approval it decides, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardChoice {
    pub approval_id: Uuid,
    pub approved: bool,
}

/// Of `actions`, in order, those shown on one card: those that may be
/// validated outside the application, as many as one card holds.
pub fn shown_in_card<'a>(
    actions: impl IntoIterator<Item = ShownAction<'a>>,
) -> Vec<ShownAction<'a>> {
    let mut shown = Vec::new();
    let mut chars = 0;
    for action in actions {
        if !validated_in_gateway(action.risk, action.parameters) {
            continue;
        }
        let (summary, _) = bounded(action.summary.trim(), MAX_ACTION_CHARS);
        let size = summary.chars().count() + exact_parameters(action.parameters).chars().count();
        if shown.len() == MAX_CARD_ACTIONS || chars + size > MAX_CARD_CHARS {
            break;
        }
        chars += size;
        shown.push(action);
    }
    shown
}

/// The Adaptive Card showing `actions`: each numbered when there are
/// several, its description (bounded) and its risk, its exact parameters
/// whole, then its buttons while it is open.
pub fn validation_card(actions: &[CardAction]) -> Value {
    let mut body = Vec::new();
    for (i, action) in actions.iter().enumerate() {
        let number = if actions.len() > 1 {
            format!(" {}", i + 1)
        } else {
            String::new()
        };
        let shown = &action.shown;
        let (summary, _) = bounded(shown.summary.trim(), MAX_ACTION_CHARS);
        body.push(runs(&[
            bold(&format!("Action{number} à valider : ")),
            plain(&summary),
        ]));
        body.push(runs(&[
            bold("Risque : "),
            plain(&format!(
                "{}. Paramètres exacts qui seront exécutés :",
                risk_label(shown.risk)
            )),
        ]));
        let parameters = exact_parameters(shown.parameters);
        body.push(runs(&[json!({
            "type": "TextRun",
            "text": parameters,
            "fontType": "Monospace",
        })]));
        let settled = match action.state {
            CardState::Open => {
                body.push(json!({
                    "type": "ActionSet",
                    "actions": [
                        button(&format!("Valider{number}"), "v", shown.approval_id),
                        button(&format!("Refuser{number}"), "r", shown.approval_id),
                    ],
                }));
                continue;
            }
            CardState::Decided => "Décidée.",
            CardState::Closed => "N’attend plus de décision.",
        };
        body.push(runs(&[json!({
            "type": "TextRun",
            "text": settled,
            "italic": true,
        })]));
    }
    json!({
        "type": "AdaptiveCard",
        "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
        "version": "1.4",
        "body": body,
    })
}

/// The choice a pressed button carries, if `value` is one of the card's.
pub fn card_choice(value: &Value) -> Option<CardChoice> {
    let approved = match value.get(KEY)?.as_str()? {
        "v" => true,
        "r" => false,
        _ => return None,
    };
    let approval = value.get("approval")?.as_str()?;
    // As the platform writes them: hyphenated, lowercase.
    let approval_id = Uuid::parse_str(approval)
        .ok()
        .filter(|id| id.hyphenated().to_string() == approval)?;
    Some(CardChoice {
        approval_id,
        approved,
    })
}

fn runs(inlines: &[Value]) -> Value {
    json!({ "type": "RichTextBlock", "inlines": inlines })
}

fn bold(text: &str) -> Value {
    json!({ "type": "TextRun", "text": text, "weight": "Bolder" })
}

fn plain(text: &str) -> Value {
    json!({ "type": "TextRun", "text": text })
}

fn button(title: &str, choice: &str, approval: Uuid) -> Value {
    json!({
        "type": "Action.Submit",
        "title": title,
        "data": { KEY: choice, "approval": approval.to_string() },
    })
}

#[cfg(test)]
#[path = "card_tests.rs"]
mod tests;
