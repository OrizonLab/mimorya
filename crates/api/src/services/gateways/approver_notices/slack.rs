//! The requests awaiting a manager told in Slack (ADR-130): a direct
//! message of the company's application, in Slack's markup
//! (`approver_text`), its actions that may be validated there with their
//! buttons (ADR-129), said without them when Slack would not show them.
//! Slack is asked first whether the linked Slack user is still the person:
//! once it no longer says so, the identity is unlinked and nothing is
//! written. `chat:write` suffices: the message goes to the Slack user's
//! identifier, in their direct conversation with the application.

use mimorya_core::gateways::{ShownAction, validated_in_gateway};
use mimorya_db::repositories::gateway_approval_notices::GatewayApprovalNotice;
use mimorya_db::repositories::gateway_replies::ReplyRecord;

use super::{Reach, Told, shown_action};
use crate::app::Services;
use crate::services::gateways::approver_text::approver_notice;
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
