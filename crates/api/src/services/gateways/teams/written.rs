//! What becomes of what the company's Teams bot writes, an answer, a pause
//! or a notice to a manager (ADR-140, ADR-141, ADR-142): a card Teams took
//! is kept, with the requests it shows, so that its buttons decide; a
//! message Teams did not take is not tried again when Teams refuses it
//! (the bot blocked or removed, the conversation unknown, the
//! application's secret refused) nor when the service is not Microsoft's,
//! and tried again a few times when Microsoft is unreachable, failing or
//! answered unread (Entra's token, `retry::later`).

use chrono::{DateTime, Utc};
use mimorya_connectors::teams::TeamsFailure;
use mimorya_db::repositories::gateway_cards::{WrittenCard, record_gateway_card};
use mimorya_db::repositories::gateway_replies::ReplyRecord;
use uuid::Uuid;

use crate::services::gateways::GatewayService;
use crate::services::gateways::retry::later;

/// Keeps the card the bot `gateway` wrote that Teams took, `written`,
/// showing `ids` with `others` said of beside it, so that its buttons
/// decide; a card Teams did not number, or not kept, decides nothing (its
/// buttons are said to be no longer valid), the message being written all
/// the same.
pub(in crate::services::gateways) async fn keep_card(
    gateways: &GatewayService,
    gateway: Uuid,
    written: Option<WrittenCard<'_>>,
    ids: &[Uuid],
    others: (i32, i32),
) {
    let Some(written) = written else {
        tracing::warn!(%gateway, "Teams card not numbered: its buttons decide nothing");
        return;
    };
    let kept = async {
        let mut tx = gateways.db.begin(written.tenant_id).await?;
        record_gateway_card(&mut tx, written, ids, others).await?;
        tx.commit().await?;
        anyhow::Ok(())
    };
    if let Err(error) = kept.await {
        let error = format!("{error:#}");
        tracing::warn!(%gateway, %error, "Teams card not kept: its buttons decide nothing");
    }
}

/// What becomes of a message Teams did not take, at its `attempt`-th try.
pub(in crate::services::gateways) fn after_failure(
    failure: &TeamsFailure,
    attempt: i32,
    now: DateTime<Utc>,
) -> ReplyRecord {
    let reason = failure.message();
    tracing::warn!(attempt, %reason, "Teams message not taken");
    match failure {
        // A refusal is a status below 500 (`Failed` holds the others).
        TeamsFailure::Refused { .. } => ReplyRecord::Done("refused"),
        TeamsFailure::Throttled { retry_after } => later(attempt, *retry_after, now),
        // A service not Microsoft's: written to no one, never tried again.
        TeamsFailure::Untrusted => ReplyRecord::Done("failed"),
        // Only Entra's answer goes unread (any message Teams takes is
        // taken, whatever it answers): nothing written yet.
        TeamsFailure::Unreadable | TeamsFailure::Failed(_) | TeamsFailure::Unanswered(_) => {
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
        // Entra's answer unread: nothing was written, tried again.
        assert_eq!(
            after_failure(&TeamsFailure::Unreadable, 2, now),
            ReplyRecord::Retry(now + Duration::minutes(2))
        );
        // A service not Microsoft's: never tried again.
        assert_eq!(
            after_failure(&TeamsFailure::Untrusted, 1, now),
            ReplyRecord::Done("failed")
        );
    }
}
