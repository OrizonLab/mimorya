//! Which actions awaiting a person's validation are validated in a
//! messaging service (ADR-129, ADR-136): only one that changes nothing
//! outside the company and can be undone, whose exact parameters fit whole
//! in a message (`validated_in_gateway`); any other, in the application,
//! which shows them whole. How such an action is shown, whatever the
//! service: its description, its risk, its exact parameters.

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
}
