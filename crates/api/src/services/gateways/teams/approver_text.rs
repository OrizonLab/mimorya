//! What a manager, or whom they named to stand in for them, is written in
//! their personal conversation with the company's Teams bot when requests
//! of a paused run await them (ADR-142): the words of `approver_text`, in
//! the Markdown Teams reads (the names escaped, whom the agent acts for in
//! bold, the link of the « Validations » screen), the actions that may be
//! validated there shown on the card the message comes with (ADR-141).
//! Written again once one of them is decided, with the names as they are
//! then; once none awaits a decision any more, that none does.

use mimorya_core::gateways::ShownAction;
use mimorya_core::gateways::teams::{CardState, escaped, link};
use mimorya_db::repositories::approvals::ApprovalRow;
use mimorya_db::repositories::gateway_cards::GatewayCard;
use mimorya_db::repositories::users::get_user;
use uuid::Uuid;

use super::choices::shown_action;
use crate::app::Services;
use crate::services::gateways::approver_text::{ApproverNotice, Written, approver_words};

/// The message telling of `notice`, its shown actions on a card.
pub fn approver_notice(notice: &ApproverNotice) -> String {
    let written = Written {
        requested_for: format!("**{}**", escaped(notice.requested_for)),
        absent: notice.absent.map(escaped),
        link: validations_link(notice.link),
    };
    approver_words(notice, &written)
}

/// What a notice's card is written again with once none of its `shown`
/// actions awaits a decision any more: each validated or refused
/// (`decided`), or one expired, or stopped with its request; and that
/// `in_application` others are to validate in the application, at `url`.
pub fn settled_notice(
    requested_for: &str,
    (shown, in_application): (usize, usize),
    decided: bool,
    url: &str,
) -> String {
    let settled = match (decided, shown == 1) {
        (true, true) => "l’action montrée ci-dessous est décidée.",
        (true, false) => "les actions montrées ci-dessous sont décidées.",
        (false, true) => "l’action montrée ci-dessous n’attend plus de décision.",
        (false, false) => "les actions montrées ci-dessous n’attendent plus de décision.",
    };
    let mut text = format!("**{}** : {}", escaped(requested_for), escaped(settled));
    if in_application > 0 {
        text.push_str(&format!(
            "{}{}{}",
            escaped(" D’autres sont à valider dans l’application ("),
            validations_link(url),
            escaped(").")
        ));
    }
    text
}

/// The text a notice's `card`, its actions `approvals` standing as
/// `states`, is written again with, whom the agent acts for and the absent
/// manager named as they are now.
pub(super) async fn rewritten_notice(
    services: &Services,
    tenant_id: Uuid,
    card: &GatewayCard,
    (approvals, states): (&[ApprovalRow], &[CardState]),
) -> anyhow::Result<String> {
    // A card shows one request at least, all of one run.
    let Some(first) = approvals.first() else {
        anyhow::bail!("a notice's card shows no request");
    };
    let mut tx = services.db.begin(tenant_id).await?;
    let requested_for = get_user(&mut tx, first.requested_for_user_id).await?;
    // Whom it awaits, when written to whom stands in for them.
    let absent = match (first.approver_user_id, card.notice_for) {
        (Some(approver), Some(to)) if approver != to => get_user(&mut tx, approver).await?,
        _ => None,
    };
    tx.commit().await?;
    let requested_for = requested_for.map(|u| u.display_name).unwrap_or_default();
    let url = services.gateways.approvals_url();
    let in_application = usize::try_from(card.in_application).unwrap_or(0);
    if !states.contains(&CardState::Open) {
        let decided = states.iter().all(|&state| state == CardState::Decided);
        let counts = (approvals.len(), in_application);
        return Ok(settled_notice(&requested_for, counts, decided, &url));
    }
    let shown: Vec<ShownAction> = approvals.iter().map(shown_action).collect();
    let absent = absent.map(|u| u.display_name);
    Ok(approver_notice(&ApproverNotice {
        requested_for: &requested_for,
        absent: absent.as_deref(),
        shown: &shown,
        in_application,
        link: &url,
    }))
}

/// The « Validations » screen's link, as Teams reads it.
fn validations_link(url: &str) -> String {
    link(url, "ouvrir les validations").unwrap_or_else(|| escaped(url))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const URL: &str = "https://mimorya.example/approvals";
    const LINK: &str = "[ouvrir les validations](https://mimorya.example/approvals)";

    #[test]
    fn a_notice_names_whom_the_agent_acts_for_in_bold_and_escaped() {
        let parameters = json!({ "proposalId": "p-1" });
        let shown = [ShownAction {
            approval_id: Uuid::new_v4(),
            summary: "Planifier",
            risk: "write",
            parameters: &parameters,
        }];
        let text = approver_notice(&ApproverNotice {
            requested_for: "Nadia *Benali*",
            absent: Some("Marc <Dupont>"),
            shown: &shown,
            in_application: 1,
            link: URL,
        });
        assert_eq!(
            text,
            format!(
                "**Nadia \\*Benali\\*** : l’agent attend votre validation pour 2 actions, en \
                 l’absence de Marc &lt;Dupont&gt;. Vous pouvez la valider ou la refuser \
                 ci-dessous, après en avoir lu les paramètres exacts, ou dans l’application \
                 ({LINK}). D’autres sont à valider dans l’application."
            )
        );
    }

    #[test]
    fn a_settled_notice_says_none_awaits_a_decision_any_more() {
        assert_eq!(
            settled_notice("Nadia", (1, 0), true, URL),
            "**Nadia** : l’action montrée ci\\-dessous est décidée\\."
        );
        assert_eq!(
            settled_notice("Nadia", (2, 1), false, URL),
            format!(
                "**Nadia** : les actions montrées ci\\-dessous n’attendent plus de \
                 décision\\. D’autres sont à valider dans l’application \\({LINK}\\)\\."
            )
        );
    }
}
