//! The requests awaiting a manager told in Slack (ADR-130): a direct
//! message of the company's application, the words of `approver_text` in
//! Slack's markup, its actions that may be validated there shown with their
//! exact parameters and buttons in its blocks (ADR-129), said without them
//! when Slack would not show them.
//! Slack is asked first whether the linked Slack user is still the person:
//! once it no longer says so, the identity is unlinked and nothing is
//! written. `chat:write` suffices: the message goes to the Slack user's
//! identifier, in their direct conversation with the application.

use mimorya_core::gateways::slack::{approval_blocks, escaped};
use mimorya_core::gateways::{ShownAction, validated_in_gateway};
use mimorya_db::repositories::gateway_approval_notices::GatewayApprovalNotice;
use mimorya_db::repositories::gateway_replies::ReplyRecord;
use serde_json::Value;

use super::{Reach, Told, shown_action};
use crate::app::Services;
use crate::services::gateways::approver_text::{ApproverNotice, Written, approver_words};
use crate::services::gateways::person::slack_says_theirs;
use crate::services::gateways::replies::after_failure;
use crate::services::gateways::reply_text::MAX_SHOWN_ACTIONS;

/// Writes `told.to`, the Slack user `reach.person`, the requests `theirs`;
/// what became of the message.
pub(super) async fn tell(
    services: &Services,
    told: &Told<'_>,
    reach: Reach<'_>,
    theirs: &[GatewayApprovalNotice],
) -> anyhow::Result<ReplyRecord> {
    let (gateway, to, token) = (told.gateway, told.to, reach.token);
    let linked = (reach.person, to.user_id);
    let verdict = slack_says_theirs(
        services,
        told.tenant_id,
        (gateway, token),
        linked,
        &to.email,
    )
    .await?;
    match verdict {
        Ok(true) => {}
        Ok(false) => return Ok(ReplyRecord::Done("skipped")),
        Err(failure) => return Ok(after_failure(&failure, reach.attempt, reach.now)),
    }
    let shown: Vec<ShownAction> = (theirs.iter())
        .filter(|r| validated_in_gateway(&r.notice.risk, &r.payload))
        .take(MAX_SHOWN_ACTIONS)
        .map(shown_action)
        .collect();
    let link = services.gateways.approvals_url();
    let notice = told.notice(theirs, &shown, &link);
    let (text, blocks) = approver_notice(&notice);
    // Their direct conversation with the application.
    let channel = reach.person;
    let slack = services.gateways.slack();
    let posted = match slack
        .post_blocks(token, channel, None, &text, blocks.as_deref())
        .await
    {
        // Buttons Slack would not show: said without them.
        Err(failure) if blocks.is_some() && failure.blocks_refused() => {
            let reason = failure.message();
            tracing::warn!(gateway = %gateway.id, %reason, "Slack refused the notice's buttons");
            let (text, _) = approver_notice(&told.notice(theirs, &[], &link));
            slack.post(token, channel, None, &text).await
        }
        posted => posted,
    };
    Ok(match posted {
        Ok(_) => ReplyRecord::Done("sent"),
        Err(failure) => after_failure(&failure, reach.attempt, reach.now),
    })
}

/// The direct message telling of `notice`, in Slack's markup: its text,
/// and the blocks showing its actions with their buttons, if any is shown.
fn approver_notice(notice: &ApproverNotice) -> (String, Option<Vec<Value>>) {
    let written = Written {
        requested_for: format!("*{}*", escaped(notice.requested_for)),
        absent: notice.absent.map(escaped),
        link: format!("<{}|ouvrir les validations>", escaped(notice.link)),
    };
    let text = approver_words(notice, &written);
    let blocks = (!notice.shown.is_empty()).then(|| approval_blocks(&text, notice.shown));
    (text, blocks)
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
        assert!(text.contains(LINK), "{text}");
        let blocks = json!(blocks.expect("its buttons")).to_string();
        assert!(blocks.contains("approval_approve"), "{blocks}");
        assert!(blocks.contains("INT-2052"), "{blocks}");
    }

    #[test]
    fn an_absence_is_said_escaped() {
        let mut told = notice(&[], 1);
        told.absent = Some("Marc <Dupont>");
        let (text, _) = approver_notice(&told);
        assert!(
            text.contains(", en l’absence de Marc &lt;Dupont&gt;."),
            "{text}"
        );
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
    }
}
