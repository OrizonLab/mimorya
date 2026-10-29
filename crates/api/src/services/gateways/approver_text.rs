//! What a manager, or whom they named to stand in for them, is written in
//! Slack when requests of a paused run await them (ADR-130), in Slack's
//! markup: for whom the agent acts, and in whose absence; the actions that
//! may be validated there, each with its exact parameters and its buttons
//! (ADR-129); how many others are to validate in the application, where
//! every request awaiting them is listed.

use mimorya_core::gateways::ShownAction;
use mimorya_core::gateways::slack::{approval_blocks, escaped};
use serde_json::Value;

/// The requests awaiting a manager, as they are told of.
pub struct ApproverNotice<'a> {
    /// For whom the agent acts.
    pub requested_for: &'a str,
    /// The absent manager's name, when a stand-in is told.
    pub absent: Option<&'a str>,
    /// The actions shown with their exact parameters and buttons.
    pub shown: &'a [ShownAction<'a>],
    /// The others, to validate in the application.
    pub in_application: usize,
    /// The « Validations » screen's address.
    pub link: &'a str,
}

/// The direct message telling of `notice`: its text, and the blocks showing
/// its actions with their buttons, if any is shown.
pub fn approver_notice(notice: &ApproverNotice) -> (String, Option<Vec<Value>>) {
    let link = format!("<{}|ouvrir les validations>", escaped(notice.link));
    let who = escaped(notice.requested_for);
    let count = notice.shown.len() + notice.in_application;
    let actions = if count == 1 {
        "une action".to_string()
    } else {
        format!("{count} actions")
    };
    let mut text = format!("*{who}* : l’agent attend votre validation pour {actions}");
    if let Some(absent) = notice.absent {
        text.push_str(&format!(", en l’absence de {}", escaped(absent)));
    }
    text.push('.');
    if notice.shown.is_empty() {
        let (pronoun, verb) = if count == 1 {
            ("Elle", "se valide")
        } else {
            ("Elles", "se valident")
        };
        text.push_str(&format!(
            " {pronoun} {verb} dans l’application, qui en montre les paramètres exacts : {link}."
        ));
        return (text, None);
    }
    let them = if notice.shown.len() == 1 {
        "la valider ou la refuser"
    } else {
        "les valider ou les refuser"
    };
    text.push_str(&format!(
        " Vous pouvez {them} ci-dessous, après en avoir lu les paramètres exacts, ou dans \
         l’application ({link})."
    ));
    if notice.in_application > 0 {
        text.push_str(" D’autres sont à valider dans l’application.");
    }
    let blocks = approval_blocks(&text, notice.shown);
    (text, Some(blocks))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    const URL: &str = "https://mimorya.example/approvals";
    const LINK: &str = "<https://mimorya.example/approvals|ouvrir les validations>";

    fn notice<'a>(shown: &'a [ShownAction<'a>], in_application: usize) -> ApproverNotice<'a> {
        ApproverNotice {
            requested_for: "Nadia <Benali>",
            absent: None,
            shown,
            in_application,
            link: URL,
        }
    }

    #[test]
    fn an_action_validated_in_slack_is_shown_with_its_buttons() {
        let parameters = json!({ "intervention": "INT-2052" });
        let action = ShownAction {
            approval_id: Uuid::new_v4(),
            summary: "Planifier INT-2052",
            risk: "write",
            parameters: &parameters,
        };
        let shown = [action];
        let (text, blocks) = approver_notice(&notice(&shown, 0));
        assert!(
            text.starts_with(
                "*Nadia &lt;Benali&gt;* : l’agent attend votre validation pour une action."
            ),
            "{text}"
        );
        assert!(
            text.contains("Vous pouvez la valider ou la refuser ci-dessous"),
            "{text}"
        );
        assert!(text.contains(LINK), "{text}");
        assert!(!text.contains("D’autres"), "{text}");
        let blocks = json!(blocks.expect("its buttons")).to_string();
        assert!(blocks.contains("approval_approve"), "{blocks}");
        assert!(blocks.contains("INT-2052"), "{blocks}");
    }

    #[test]
    fn the_others_and_an_absence_are_said() {
        let parameters = json!({});
        let action = ShownAction {
            approval_id: Uuid::new_v4(),
            summary: "Planifier",
            risk: "write",
            parameters: &parameters,
        };
        let shown = [action, action];
        let mut told = notice(&shown, 1);
        told.absent = Some("Marc Dupont");
        let (text, blocks) = approver_notice(&told);
        assert!(
            text.contains("pour 3 actions, en l’absence de Marc Dupont."),
            "{text}"
        );
        assert!(
            text.contains("Vous pouvez les valider ou les refuser"),
            "{text}"
        );
        assert!(
            text.ends_with("D’autres sont à valider dans l’application."),
            "{text}"
        );
        assert!(blocks.is_some());
    }

    #[test]
    fn actions_validated_in_the_application_are_shown_without_buttons() {
        let (one, blocks) = approver_notice(&notice(&[], 1));
        assert_eq!(
            one,
            format!(
                "*Nadia &lt;Benali&gt;* : l’agent attend votre validation pour une action. Elle \
                 se valide dans l’application, qui en montre les paramètres exacts : {LINK}."
            )
        );
        assert_eq!(blocks, None);
        let (two, _) = approver_notice(&notice(&[], 2));
        assert!(two.contains("pour 2 actions. Elles se valident"), "{two}");
    }
}
