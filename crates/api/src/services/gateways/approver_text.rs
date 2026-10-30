//! What a manager, or whom they named to stand in for them, is told when
//! requests of a paused run await them, in any messaging service (ADR-130,
//! ADR-137): for whom the agent acts, and in whose absence; that the
//! actions shown may be validated there, after their exact parameters are
//! read, or in the application; how many others are to validate in the
//! application, where every request awaiting them is listed. The same
//! words in each service, each writing the names and the link as it reads
//! them (`Written`) and showing the actions its own way: Slack's blocks
//! (`approver_notices/slack.rs`), Telegram's text and buttons
//! (`telegram::approver_notice`).

use mimorya_core::gateways::ShownAction;

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

/// A notice's names and link, as a service writes them: escaped, the name
/// of whom the agent acts for set apart.
pub struct Written {
    pub requested_for: String,
    pub absent: Option<String>,
    pub link: String,
}

/// What `notice` says, its names and link as `written`.
pub fn approver_words(notice: &ApproverNotice, written: &Written) -> String {
    let count = notice.shown.len() + notice.in_application;
    let actions = if count == 1 {
        "une action".to_owned()
    } else {
        format!("{count} actions")
    };
    let (who, link) = (&written.requested_for, &written.link);
    let mut text = format!("{who} : l’agent attend votre validation pour {actions}");
    if let Some(absent) = &written.absent {
        text.push_str(&format!(", en l’absence de {absent}"));
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
        return text;
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
    text
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn words(shown: &[ShownAction], in_application: usize, absent: Option<&str>) -> String {
        let notice = ApproverNotice {
            requested_for: "Nadia",
            absent,
            shown,
            in_application,
            link: "lien",
        };
        let written = Written {
            requested_for: "Nadia".into(),
            absent: absent.map(str::to_owned),
            link: "LIEN".into(),
        };
        approver_words(&notice, &written)
    }

    #[test]
    fn the_actions_shown_are_validated_below_the_others_in_the_application() {
        let parameters = json!({});
        let action = ShownAction {
            approval_id: Uuid::new_v4(),
            summary: "Planifier",
            risk: "write",
            parameters: &parameters,
        };
        assert_eq!(
            words(&[action], 0, None),
            "Nadia : l’agent attend votre validation pour une action. Vous pouvez la valider ou \
             la refuser ci-dessous, après en avoir lu les paramètres exacts, ou dans \
             l’application (LIEN)."
        );
        assert_eq!(
            words(&[action, action], 1, Some("Marc")),
            "Nadia : l’agent attend votre validation pour 3 actions, en l’absence de Marc. Vous \
             pouvez les valider ou les refuser ci-dessous, après en avoir lu les paramètres \
             exacts, ou dans l’application (LIEN). D’autres sont à valider dans l’application."
        );
    }

    #[test]
    fn actions_validated_in_the_application_are_said_so() {
        assert_eq!(
            words(&[], 1, None),
            "Nadia : l’agent attend votre validation pour une action. Elle se valide dans \
             l’application, qui en montre les paramètres exacts : LIEN."
        );
        assert!(words(&[], 2, None).contains("pour 2 actions. Elles se valident"));
    }
}
