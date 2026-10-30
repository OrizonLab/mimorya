//! An end or pause answered in the person's personal conversation with the
//! company's Teams bot (ADR-140), where its conversation is held, in the
//! Markdown Teams reads (`teams::reply_text`), at the Teams service its
//! latest message came from, as the bot's application. A pause shows the
//! actions awaiting the person's own validation that may be given there on
//! a card, with their exact parameters and their buttons (ADR-141), as many
//! as one card holds (`shown_in_card`); the card is kept, with the requests
//! it shows, once Teams took it. The others are validated in the
//! application, which shows them whole. What Teams refuses (the bot blocked
//! or removed, the conversation unknown, the application's secret refused)
//! is not tried again, nor a service that is not Microsoft's; Microsoft
//! unreachable, failing or answered unread (Entra's token), tried again a
//! few times (`retry::later`).

use chrono::{DateTime, Utc};
use mimorya_connectors::teams::TeamsFailure;
use mimorya_core::gateways::teams::{
    CardAction, CardState, TeamsApplication, shown_in_card, validation_card,
};
use mimorya_db::repositories::approvals::ApprovalRow;
use mimorya_db::repositories::gateway_cards::{GatewayCard, record_gateway_card};
use mimorya_db::repositories::gateway_replies::{ReplyRecord, ReplyTarget};
use uuid::Uuid;

use crate::services::gateways::GatewayService;
use crate::services::gateways::retry::later;
use crate::services::gateways::teams::{TeamsPause, shown_action, teams_reply_text};

/// Writes the answer to `target`, of the company `tenant_id`, in the
/// person's personal conversation with the bot, whose application's secret
/// is `client_secret`; `awaiting`, a pause's actions still awaiting a
/// decision. What became of it.
pub(super) async fn answer(
    gateways: &GatewayService,
    (tenant_id, target): (Uuid, &ReplyTarget),
    awaiting: &[ApprovalRow],
    client_secret: String,
    now: DateTime<Utc>,
) -> ReplyRecord {
    // A Teams thread always has one (the base requires it).
    let Some(service_url) = target.service_url.as_deref() else {
        tracing::warn!(gateway = %target.gateway_id, "Teams conversation's service unknown");
        return ReplyRecord::Done("failed");
    };
    let own: Vec<&ApprovalRow> = (awaiting.iter())
        .filter(|a| !a.routed_to_manager())
        .collect();
    let paused = target.status == "awaiting_approval";
    let shown = if paused {
        shown_in_card(own.iter().map(|a| shown_action(a)))
    } else {
        Vec::new()
    };
    let pause = TeamsPause {
        shown: shown.len(),
        in_application: own.len() - shown.len(),
        managed: awaiting.len() - own.len(),
    };
    let url = gateways.conversation_url(target.conversation_id);
    let text = teams_reply_text(&target.status, target.answer.as_deref(), &url, pause);
    let app = TeamsApplication {
        app_id: target.bot_user_id.clone(),
        directory_id: target.workspace_id.clone(),
        client_secret,
    };
    let to = (service_url, target.channel_id.as_str());
    let teams = gateways.teams();
    if shown.is_empty() {
        return match teams.send(&app, to, &text).await {
            Ok(()) => ReplyRecord::Done("sent"),
            Err(failure) => after_failure(&failure, target.attempts + 1, now),
        };
    }
    let actions: Vec<CardAction> = (shown.iter())
        .map(|&shown| CardAction {
            shown,
            state: CardState::Open,
        })
        .collect();
    let card = GatewayCard {
        approval_ids: shown.iter().map(|a| a.approval_id).collect(),
        in_application: i32::try_from(pause.in_application).unwrap_or(i32::MAX),
        managed: i32::try_from(pause.managed).unwrap_or(i32::MAX),
        conversation_id: target.conversation_id,
    };
    match teams
        .send_card(&app, to, &text, &validation_card(&actions))
        .await
    {
        Ok(activity) => {
            keep_card(gateways, (tenant_id, target), activity.as_deref(), &card).await;
            ReplyRecord::Done("sent")
        }
        Err(failure) => after_failure(&failure, target.attempts + 1, now),
    }
}

/// Keeps the card Teams took as `activity`, so that its buttons decide; a
/// card Teams did not number, or not kept, decides nothing (its buttons
/// are said to be no longer valid), the message being written all the same.
async fn keep_card(
    gateways: &GatewayService,
    (tenant_id, target): (Uuid, &ReplyTarget),
    activity: Option<&str>,
    card: &GatewayCard,
) {
    let gateway = target.gateway_id;
    let Some(activity) = activity else {
        tracing::warn!(%gateway, "Teams card not numbered: its buttons decide nothing");
        return;
    };
    let kept = async {
        let mut tx = gateways.db.begin(tenant_id).await?;
        record_gateway_card(&mut tx, (tenant_id, target.thread_id), activity, card).await?;
        tx.commit().await?;
        anyhow::Ok(())
    };
    if let Err(error) = kept.await {
        let error = format!("{error:#}");
        tracing::warn!(%gateway, %error, "Teams card not kept: its buttons decide nothing");
    }
}

/// What becomes of an answer Teams did not take, at its `attempt`-th try.
fn after_failure(failure: &TeamsFailure, attempt: i32, now: DateTime<Utc>) -> ReplyRecord {
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
