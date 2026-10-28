//! The answers in Slack (ADR-127): when a run a person started there ends
//! or pauses (its `run_finished` event), what the conversation shows of it
//! is written in the thread the conversation is held in (`reply_text`).
//! Whichever path ended it: its outcome, a validation decided, the stale-run
//! sweep, a stop. Each end or pause is answered once, by the pass of a
//! process that executes runs, one at a time (ADR-086), within
//! `ANSWERED_WITHIN` of it. A paused gateway, or a Slack user no longer the
//! person's identity, is written nothing. What Slack refuses is recorded
//! and not tried again; Slack unreachable, tried again a few times, then
//! given up. Each is written once, unless the service stops between the
//! message sent and its record. Nothing written is logged nor recorded: it
//! is in the conversation.

use chrono::{DateTime, Duration, Utc};
use mimorya_connectors::slack::SlackFailure;
use mimorya_db::repositories::gateway_replies::{
    ReplyRecord, UnrepliedEvent, gateway_reply_target, record_gateway_reply,
    unreplied_gateway_events,
};

use super::bot_token_name;
use super::reply_text::reply_text;
use crate::app::Services;

/// How long after a run's end or pause it is still answered: one that
/// Slack could not be reached for is not, days later.
pub const ANSWERED_WITHIN: Duration = Duration::hours(24);
/// The attempts an answer is given while Slack cannot be reached.
pub const MAX_ATTEMPTS: i32 = 4;
/// The ends and pauses a pass answers, at most.
const PER_PASS: i32 = 50;
/// The pass's lock (ADR-086).
const REPLIES_SWEEP: &str = "gateway-replies";

/// Answers in Slack the ends and pauses since `ANSWERED_WITHIN` still to
/// answer; how many were sent. Only in a process that executes runs, and
/// one at a time: none while another answers them.
pub async fn reply_to_finished_runs(services: &Services) -> anyhow::Result<usize> {
    if !services.runs.executes() {
        return Ok(0);
    }
    // Most passes find nothing: the lock, on a connection of its own, is
    // taken only when there is something to answer.
    let now = Utc::now();
    let pool = services.db.pool();
    if unreplied_gateway_events(pool, now - ANSWERED_WITHIN, now, 1)
        .await?
        .is_empty()
    {
        return Ok(0);
    }
    let Some(sweep) = services.db.try_sweep_lock(REPLIES_SWEEP).await? else {
        return Ok(0);
    };
    let sent = reply_since(services, Utc::now()).await;
    sweep.release().await;
    sent
}

async fn reply_since(services: &Services, now: DateTime<Utc>) -> anyhow::Result<usize> {
    let since = now - ANSWERED_WITHIN;
    let pending = unreplied_gateway_events(services.db.pool(), since, now, PER_PASS).await?;
    let mut sent = 0;
    for event in pending {
        match reply(services, event, now).await {
            Ok(true) => sent += 1,
            Ok(false) => {}
            Err(error) => {
                let event = event.event_id;
                tracing::warn!(event, error = format!("{error:#}"), "Slack answer failed");
            }
        }
    }
    Ok(sent)
}

/// Answers `event` in its thread; whether it was sent.
async fn reply(
    services: &Services,
    event: UnrepliedEvent,
    now: DateTime<Utc>,
) -> anyhow::Result<bool> {
    let gateways = &services.gateways;
    let mut tx = services.db.begin(event.tenant_id).await?;
    let Some(target) = gateway_reply_target(&mut tx, event.event_id).await? else {
        return Ok(false);
    };
    let (tenant_id, event_id, thread_id) = (event.tenant_id, target.event_id, target.thread_id);
    if !target.gateway_enabled || !target.still_linked {
        let skipped = ReplyRecord::Done("skipped");
        record_gateway_reply(&mut tx, tenant_id, event_id, thread_id, skipped).await?;
        tx.commit().await?;
        return Ok(false);
    }
    let token = gateways
        .secret(&mut tx, tenant_id, &bot_token_name(target.gateway_id))
        .await?;
    tx.commit().await?;
    let record = match token {
        // No master key, or another one: told the operator.
        None => {
            let gateway = target.gateway_id;
            tracing::warn!(%gateway, "Slack gateway's bot token unreadable");
            ReplyRecord::Done("failed")
        }
        Some(token) => {
            let url = gateways.conversation_url(target.conversation_id);
            let text = reply_text(&target.status, target.answer.as_deref(), &url);
            let thread = Some(target.thread_ts.as_str());
            match gateways
                .slack()
                .post(&token, &target.channel_id, thread, &text)
                .await
            {
                Ok(_) => ReplyRecord::Done("sent"),
                Err(failure) => after_failure(&failure, target.attempts + 1, now),
            }
        }
    };
    let mut tx = services.db.begin(tenant_id).await?;
    record_gateway_reply(&mut tx, tenant_id, event_id, thread_id, record).await?;
    tx.commit().await?;
    Ok(record == ReplyRecord::Done("sent"))
}

/// What becomes of an answer Slack did not take, at its `attempt`-th try:
/// refused, never tried again; unreached, tried again later, a few times.
fn after_failure(failure: &SlackFailure, attempt: i32, now: DateTime<Utc>) -> ReplyRecord {
    let reason = failure.message();
    tracing::warn!(attempt, %reason, "Slack answer not taken");
    if matches!(failure, SlackFailure::Refused(_)) {
        return ReplyRecord::Done("refused");
    }
    if attempt >= MAX_ATTEMPTS {
        return ReplyRecord::Done("failed");
    }
    // 30 s, 2 min, 8 min; or when Slack says.
    let wait = match failure {
        SlackFailure::Throttled {
            retry_after: Some(after),
        } => Duration::from_std(*after).unwrap_or(Duration::minutes(1)),
        _ => Duration::seconds(30 * 4_i64.pow(attempt.saturating_sub(1) as u32)),
    };
    ReplyRecord::Retry(now + wait)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slack_unreached_is_tried_again_later_and_refusing_never() {
        let now = Utc::now();
        let unreached = SlackFailure::Unanswered("trop lent");
        assert_eq!(
            after_failure(&unreached, 1, now),
            ReplyRecord::Retry(now + Duration::seconds(30))
        );
        assert_eq!(
            after_failure(&unreached, 3, now),
            ReplyRecord::Retry(now + Duration::minutes(8))
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
