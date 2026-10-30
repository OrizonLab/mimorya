//! An end or pause answered in the person's personal conversation with the
//! company's Teams bot (ADR-140), where its conversation is held, in the
//! Markdown Teams reads (`teams::reply_text`), at the Teams service its
//! latest message came from, as the bot's application. A pause says whose
//! validation it awaits, to give in the application, which shows the
//! actions' exact parameters. What Teams refuses (the bot blocked or
//! removed, the conversation unknown, the application's secret refused) is
//! not tried again; Microsoft unreachable or failing, tried again a few
//! times (`retry::later`).

use chrono::{DateTime, Utc};
use mimorya_connectors::teams::TeamsFailure;
use mimorya_core::gateways::teams::TeamsApplication;
use mimorya_db::repositories::approvals::ApprovalRow;
use mimorya_db::repositories::gateway_replies::{ReplyRecord, ReplyTarget};

use crate::services::gateways::GatewayService;
use crate::services::gateways::retry::later;
use crate::services::gateways::teams::{TeamsPause, teams_reply_text};

/// Writes the answer to `target` in the person's personal conversation
/// with the bot, whose application's secret is `client_secret`; `awaiting`,
/// a pause's actions still awaiting a decision. What became of it.
pub(super) async fn answer(
    gateways: &GatewayService,
    target: &ReplyTarget,
    awaiting: &[ApprovalRow],
    client_secret: String,
    now: DateTime<Utc>,
) -> ReplyRecord {
    let Some(service_url) = target.service_url.as_deref() else {
        tracing::warn!(gateway = %target.gateway_id, "Teams conversation's service unknown");
        return ReplyRecord::Done("failed");
    };
    let own = awaiting.iter().filter(|a| !a.routed_to_manager()).count();
    let pause = TeamsPause {
        own,
        managed: awaiting.len() - own,
    };
    let url = gateways.conversation_url(target.conversation_id);
    let text = teams_reply_text(&target.status, target.answer.as_deref(), &url, pause);
    let app = TeamsApplication {
        app_id: target.bot_user_id.clone(),
        directory_id: target.workspace_id.clone(),
        client_secret,
    };
    let to = (service_url, target.channel_id.as_str());
    match gateways.teams().send(&app, to, &text).await {
        Ok(()) => ReplyRecord::Done("sent"),
        Err(failure) => after_failure(&failure, target.attempts + 1, now),
    }
}

/// What becomes of an answer Teams did not take, at its `attempt`-th try.
fn after_failure(failure: &TeamsFailure, attempt: i32, now: DateTime<Utc>) -> ReplyRecord {
    let reason = failure.message();
    tracing::warn!(attempt, %reason, "Teams message not taken");
    match failure {
        TeamsFailure::Refused { status, .. } if *status < 500 => ReplyRecord::Done("refused"),
        TeamsFailure::Throttled { retry_after } => later(attempt, *retry_after, now),
        // Written to no one: never tried again. An answer not read may
        // have been taken: not written twice.
        TeamsFailure::Untrusted | TeamsFailure::Unreadable => ReplyRecord::Done("failed"),
        TeamsFailure::Refused { .. } | TeamsFailure::Failed(_) | TeamsFailure::Unanswered(_) => {
            later(attempt, None, now)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gateways::retry::MAX_ATTEMPTS;
    use chrono::Duration;
    use mimorya_connectors::teams::RefusalCause;

    #[test]
    fn microsoft_unreached_is_tried_again_later_and_a_refusal_never() {
        let now = Utc::now();
        let unreached = TeamsFailure::Unanswered("trop lent");
        assert_eq!(
            after_failure(&unreached, 1, now),
            ReplyRecord::Retry(now + Duration::seconds(30))
        );
        assert_eq!(
            after_failure(&TeamsFailure::Failed(502), MAX_ATTEMPTS, now),
            ReplyRecord::Done("failed")
        );
        let blocked = TeamsFailure::Refused {
            status: 403,
            cause: RefusalCause::Forbidden,
        };
        assert_eq!(
            after_failure(&blocked, 1, now),
            ReplyRecord::Done("refused")
        );
        let throttled = TeamsFailure::Throttled {
            retry_after: Some(std::time::Duration::from_secs(12)),
        };
        assert_eq!(
            after_failure(&throttled, 1, now),
            ReplyRecord::Retry(now + Duration::seconds(12))
        );
        assert_eq!(
            after_failure(&TeamsFailure::Unreadable, 1, now),
            ReplyRecord::Done("failed")
        );
    }
}
