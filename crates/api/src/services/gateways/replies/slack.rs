//! An end or pause answered in the Slack thread its conversation is held
//! in (ADR-127), in Slack's markup (`reply_text`): a pause shows the
//! actions awaiting the person's own validation with their buttons
//! (ADR-129), said without them when Slack would not show them.

use chrono::{DateTime, Utc};
use mimorya_connectors::slack::SlackFailure;
use mimorya_core::gateways::{ShownAction, validated_in_gateway};
use mimorya_db::repositories::approvals::ApprovalRow;
use mimorya_db::repositories::gateway_replies::{ReplyRecord, ReplyTarget};

use super::later;
use crate::services::gateways::GatewayService;
use crate::services::gateways::reply_text::{Pause, pause_reply, reply_text};

/// Writes the answer to `target` in its thread, with the bot's `token`;
/// `awaiting`, a pause's actions still awaiting a decision. What became of
/// it.
pub(super) async fn answer(
    gateways: &GatewayService,
    target: &ReplyTarget,
    awaiting: &[ApprovalRow],
    token: &str,
    now: DateTime<Utc>,
) -> ReplyRecord {
    let url = gateways.conversation_url(target.conversation_id);
    let (text, blocks) = if target.status == "awaiting_approval" {
        let theirs: Vec<_> = (awaiting.iter())
            .filter(|a| !a.routed_to_manager())
            .collect();
        let own: Vec<_> = (theirs.iter())
            .filter(|a| validated_in_gateway(&a.risk, &a.payload))
            .map(|a| ShownAction {
                approval_id: a.id,
                summary: &a.action_summary,
                risk: &a.risk,
                parameters: &a.payload,
            })
            .collect();
        let pause = Pause {
            own: &own,
            in_application: theirs.len() - own.len(),
            managed: awaiting.len() - theirs.len(),
        };
        pause_reply(&url, &pause)
    } else {
        (
            reply_text(&target.status, target.answer.as_deref(), &url),
            None,
        )
    };
    let thread = Some(target.thread_ts.as_str());
    let slack = gateways.slack();
    let channel = &target.channel_id;
    let posted = match slack
        .post_blocks(token, channel, thread, &text, blocks.as_deref())
        .await
    {
        // Buttons Slack would not show: the pause said without them.
        Err(failure) if blocks.is_some() && failure.blocks_refused() => {
            let (gateway, reason) = (target.gateway_id, failure.message());
            tracing::warn!(%gateway, %reason, "Slack refused the pause's buttons");
            let text = reply_text(&target.status, None, &url);
            slack.post(token, channel, thread, &text).await
        }
        posted => posted,
    };
    match posted {
        Ok(_) => ReplyRecord::Done("sent"),
        Err(failure) => after_failure(&failure, target.attempts + 1, now),
    }
}

/// What becomes of an answer Slack did not take, at its `attempt`-th try:
/// refused, never tried again; unreached, tried again later, a few times.
pub(in crate::services::gateways) fn after_failure(
    failure: &SlackFailure,
    attempt: i32,
    now: DateTime<Utc>,
) -> ReplyRecord {
    let reason = failure.message();
    tracing::warn!(attempt, %reason, "Slack answer not taken");
    match failure {
        SlackFailure::Refused(_) => ReplyRecord::Done("refused"),
        SlackFailure::Throttled { retry_after } => later(attempt, *retry_after, now),
        _ => later(attempt, None, now),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gateways::replies::MAX_ATTEMPTS;
    use chrono::Duration;

    #[test]
    fn slack_unreached_is_tried_again_later_and_refusing_never() {
        let now = Utc::now();
        let unreached = SlackFailure::Unanswered("trop lent");
        assert_eq!(
            after_failure(&unreached, 1, now),
            ReplyRecord::Retry(now + Duration::seconds(30))
        );
        assert_eq!(
            after_failure(&unreached, MAX_ATTEMPTS, now),
            ReplyRecord::Done("failed")
        );
        let throttled = SlackFailure::Throttled {
            retry_after: Some(std::time::Duration::from_secs(12)),
        };
        assert_eq!(
            after_failure(&throttled, 1, now),
            ReplyRecord::Retry(now + Duration::seconds(12))
        );
        let refused = SlackFailure::Refused("channel_not_found".into());
        assert_eq!(
            after_failure(&refused, 1, now),
            ReplyRecord::Done("refused")
        );
    }
}
