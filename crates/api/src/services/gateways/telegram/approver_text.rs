//! What a manager, or whom they named to stand in for them, is written in
//! their private chat with the company's Telegram bot when requests of a
//! paused run await them (ADR-137), in the HTML Telegram reads, or in plain
//! text once Telegram refused it: for whom the agent acts, and in whose
//! absence; the actions that may be validated there, each with its exact
//! parameters, whole, its buttons under the message (ADR-136); how many
//! others are to validate in the application, where every request awaiting
//! them is listed.

use mimorya_core::gateways::telegram::{escaped, link, shown_actions};

use super::reply_text::Markup;
use crate::services::gateways::approver_text::ApproverNotice;

/// The message telling of `notice`, in `markup`.
pub fn approver_notice(markup: Markup, notice: &ApproverNotice) -> String {
    let html = markup == Markup::Html;
    let (who, absent, link) = if html {
        (
            format!("<b>{}</b>", escaped(notice.requested_for)),
            notice.absent.map(escaped),
            link(notice.link, "ouvrir les validations"),
        )
    } else {
        (
            notice.requested_for.to_owned(),
            notice.absent.map(str::to_owned),
            notice.link.to_owned(),
        )
    };
    let count = notice.shown.len() + notice.in_application;
    let actions = if count == 1 {
        "une action".to_owned()
    } else {
        format!("{count} actions")
    };
    let mut text = format!("{who} : l’agent attend votre validation pour {actions}");
    if let Some(absent) = absent {
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
    text.push_str(&shown_actions(notice.shown, html));
    text
}

#[cfg(test)]
mod tests {
    use mimorya_core::gateways::ShownAction;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    const URL: &str = "https://mimorya.example/approvals";
    const LINK: &str = "<a href=\"https://mimorya.example/approvals\">ouvrir les validations</a>";

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
    fn an_action_validated_in_telegram_is_shown_with_its_exact_parameters() {
        let parameters = json!({ "intervention": "<INT-2052>" });
        let action = ShownAction {
            approval_id: Uuid::new_v4(),
            summary: "Planifier INT-2052",
            risk: "write",
            parameters: &parameters,
        };
        let shown = [action];
        let text = approver_notice(Markup::Html, &notice(&shown, 0));
        assert!(
            text.starts_with(&format!(
                "<b>Nadia &lt;Benali&gt;</b> : l’agent attend votre validation pour une action. \
                 Vous pouvez la valider ou la refuser ci-dessous, après en avoir lu les \
                 paramètres exacts, ou dans l’application ({LINK})."
            )),
            "{text}"
        );
        assert!(!text.contains("D’autres"), "{text}");
        assert!(
            text.contains("<b>Action à valider :</b> Planifier INT-2052"),
            "{text}"
        );
        assert!(text.contains("&lt;INT-2052&gt;"), "{text}");
        // In plain text, as written, with the address itself.
        let plain = approver_notice(Markup::Plain, &notice(&shown, 0));
        assert!(
            plain.starts_with("Nadia <Benali> : l’agent attend votre validation"),
            "{plain}"
        );
        assert!(plain.contains(&format!("({URL})")), "{plain}");
        assert!(plain.contains("<INT-2052>"), "{plain}");
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
        told.absent = Some("Marc <Lefèvre>");
        let text = approver_notice(Markup::Html, &told);
        assert!(
            text.contains("pour 3 actions, en l’absence de Marc &lt;Lefèvre&gt;."),
            "{text}"
        );
        assert!(
            text.contains("Vous pouvez les valider ou les refuser"),
            "{text}"
        );
        assert!(
            text.contains("). D’autres sont à valider dans l’application.\n\n<b>Action 1"),
            "{text}"
        );
    }

    #[test]
    fn actions_validated_in_the_application_are_said_without_their_parameters() {
        let one = approver_notice(Markup::Html, &notice(&[], 1));
        assert_eq!(
            one,
            format!(
                "<b>Nadia &lt;Benali&gt;</b> : l’agent attend votre validation pour une action. \
                 Elle se valide dans l’application, qui en montre les paramètres exacts : {LINK}."
            )
        );
        let two = approver_notice(Markup::Plain, &notice(&[], 2));
        assert_eq!(
            two,
            format!(
                "Nadia <Benali> : l’agent attend votre validation pour 2 actions. Elles se \
                 valident dans l’application, qui en montre les paramètres exacts : {URL}."
            )
        );
    }
}
