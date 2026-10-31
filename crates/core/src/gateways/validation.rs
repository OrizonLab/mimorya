//! Which actions awaiting a person's validation are validated in a
//! messaging service (ADR-129, ADR-136): only one that changes nothing
//! outside the company and can be undone, whose exact parameters fit whole
//! in a message (`validated_in_gateway`); any other, in the application,
//! which shows them whole. How such an action is shown, whatever the
//! service: its description, its risk, its exact parameters; once settled,
//! no longer as one to validate, but with what became of it (ADR-147).

use serde_json::Value;
use uuid::Uuid;

use crate::autonomy::ToolRisk;

/// The most of an action's description shown in a message.
pub const MAX_ACTION_CHARS: usize = 500;
/// The most of an action's exact parameters a message shows: beyond, it is
/// validated in the application, which shows them whole.
pub const MAX_PARAMETERS_CHARS: usize = 2_500;

/// An action awaiting a person's validation, shown in a messaging service.
#[derive(Debug, Clone, Copy)]
pub struct ShownAction<'a> {
    pub approval_id: Uuid,
    /// What the agent says it does.
    pub summary: &'a str,
    /// Its risk, as stored (`write`…).
    pub risk: &'a str,
    /// Its exact parameters, as they will be carried out.
    pub parameters: &'a Value,
}

/// Whether an action of `risk` with these exact `parameters` may be
/// validated in a messaging service: never one that sends something
/// outside the company or cannot be undone (validated in the application,
/// ADR-119, ADR-123), nor one whose exact parameters do not fit whole in a
/// message.
pub fn validated_in_gateway(risk: &str, parameters: &Value) -> bool {
    let undone = matches!(
        ToolRisk::parse(risk),
        Some(ToolRisk::Read | ToolRisk::Draft | ToolRisk::Write)
    );
    undone && exact_parameters(parameters).chars().count() <= MAX_PARAMETERS_CHARS
}

/// The exact parameters, as a message shows them: indented JSON.
pub fn exact_parameters(parameters: &Value) -> String {
    serde_json::to_string_pretty(parameters).unwrap_or_else(|_| parameters.to_string())
}

/// The risk of an action, in French.
pub fn risk_label(risk: &str) -> &'static str {
    match ToolRisk::parse(risk) {
        Some(ToolRisk::Read) => "Lecture",
        Some(ToolRisk::Draft) => "Brouillon",
        Some(ToolRisk::Write) => "Modification",
        Some(ToolRisk::External) => "Impact externe",
        Some(ToolRisk::Irreversible) | None => "Irréversible",
    }
}

/// Where an action shown in a message stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionState {
    /// Awaiting a decision: its buttons shown.
    Open,
    Approved,
    Rejected,
    /// Awaiting nothing more, undecided: expired, or its request stopped.
    Closed,
}

impl ActionState {
    /// Where an action whose request's status is `status` stands.
    pub fn of(status: &str) -> Self {
        match status {
            "pending" => Self::Open,
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            _ => Self::Closed,
        }
    }

    /// Validated or refused.
    pub fn is_decided(self) -> bool {
        matches!(self, Self::Approved | Self::Rejected)
    }

    /// What became of it, said under it once settled; `None` while open.
    pub fn outcome(self) -> Option<&'static str> {
        match self {
            Self::Open => None,
            Self::Approved => Some("Validée."),
            Self::Rejected => Some("Refusée."),
            Self::Closed => Some("N’attend plus de décision."),
        }
    }

    /// The heading of the action `number` (« 1 », or empty when it is the
    /// only one): to validate while open, an action once settled.
    pub fn heading(self, number: &str) -> String {
        let number = if number.is_empty() {
            String::new()
        } else {
            format!(" {number}")
        };
        match self {
            Self::Open => format!("Action{number} à valider :"),
            _ => format!("Action{number} :"),
        }
    }

    /// What its exact parameters are: to be carried out while open.
    pub fn parameters_heading(self) -> &'static str {
        match self {
            Self::Open => "Paramètres exacts qui seront exécutés :",
            _ => "Paramètres exacts :",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_an_action_that_can_be_undone_with_parameters_that_fit_is_validated_there() {
        let small = json!({ "proposalId": "p-1" });
        assert!(validated_in_gateway("read", &small));
        assert!(validated_in_gateway("draft", &small));
        assert!(validated_in_gateway("write", &small));
        assert!(!validated_in_gateway("external", &small));
        assert!(!validated_in_gateway("irreversible", &small));
        assert!(!validated_in_gateway("inconnu", &small));
        let large = json!({ "body": "x".repeat(MAX_PARAMETERS_CHARS) });
        assert!(!validated_in_gateway("write", &large));
    }

    #[test]
    fn a_risk_is_named_in_french_an_unknown_one_as_the_worst() {
        assert_eq!(risk_label("write"), "Modification");
        assert_eq!(risk_label("external"), "Impact externe");
        assert_eq!(risk_label("inconnu"), "Irréversible");
    }

    #[test]
    fn a_settled_action_is_no_longer_one_to_validate_and_says_what_became_of_it() {
        assert_eq!(ActionState::of("pending"), ActionState::Open);
        assert_eq!(ActionState::of("approved"), ActionState::Approved);
        assert_eq!(ActionState::of("rejected"), ActionState::Rejected);
        assert_eq!(ActionState::of("expired"), ActionState::Closed);
        assert_eq!(ActionState::of("cancelled"), ActionState::Closed);
        assert!(ActionState::Approved.is_decided() && ActionState::Rejected.is_decided());
        assert!(!ActionState::Closed.is_decided() && !ActionState::Open.is_decided());

        assert_eq!(ActionState::Open.heading(""), "Action à valider :");
        assert_eq!(ActionState::Open.heading("2"), "Action 2 à valider :");
        assert_eq!(
            ActionState::Open.parameters_heading(),
            "Paramètres exacts qui seront exécutés :"
        );
        assert_eq!(ActionState::Open.outcome(), None);
        for (state, outcome) in [
            (ActionState::Approved, "Validée."),
            (ActionState::Rejected, "Refusée."),
            (ActionState::Closed, "N’attend plus de décision."),
        ] {
            assert_eq!(state.heading("2"), "Action 2 :");
            assert_eq!(state.parameters_heading(), "Paramètres exacts :");
            assert_eq!(state.outcome(), Some(outcome));
        }
    }
}
