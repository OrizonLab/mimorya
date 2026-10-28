//! The answers in Slack (ADR-127): when a run a person started there ends
//! or pauses (its `run_finished` event), what the conversation shows of it
//! is written in the thread the conversation is held in (`reply_text`).
//! Whichever path ended it: its outcome, a validation decided, the stale-run
//! sweep, a stop. Each end or pause is answered once, by the pass of a
//! process that executes runs, one at a time (ADR-086), within
//! `ANSWERED_WITHIN` of it, in a thread active within `THREAD_ACTIVE_WITHIN`;
//! a thread's answers in order: one waiting to be tried again, or not
//! recorded, holds back those after it while it is still answered (one too
//! old is no longer tried, its outcome left empty, and holds back none).
//! A paused gateway, or a Slack user no longer the person's identity, is
//! written nothing. What Slack refuses is recorded and not tried again;
//! Slack unreachable, tried again a few times, then given up.
//! An answer is written twice when Slack took it but answered too late, or
//! the service stopped between the message sent and its record. Nothing
//! written is logged nor recorded: it is in the conversation.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use mimorya_connectors::slack::SlackFailure;
use mimorya_db::repositories::approvals::list_approvals_for_run;
use mimorya_db::repositories::gateway_replies::{
    ReplyRecord, UnrepliedEvent, gateway_reply_target, record_gateway_reply,
    unreplied_gateway_events,
};
use uuid::Uuid;

use super::bot_token_name;
use super::reply_text::{Pause, pause_reply, reply_text};
use crate::app::Services;

/// How long after a run's end or pause it is still answered: one that
/// Slack could not be reached for is not, days later.
pub const ANSWERED_WITHIN: Duration = Duration::hours(24);
/// The threads whose answers are looked for: a turn or an answer there
/// within this delay. A run's next end comes within a validation's delay
/// (seven days) of the latest; a margin above it.
const THREAD_ACTIVE_WITHIN: Duration = Duration::days(30);
/// The longest wait before an answer is tried again, whatever Slack says.
const MAX_WAIT: Duration = Duration::hours(1);
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
    let (active, since) = (now - THREAD_ACTIVE_WITHIN, now - ANSWERED_WITHIN);
    if unreplied_gateway_events(pool, active, since, now, 1)
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
    let (active, since) = (now - THREAD_ACTIVE_WITHIN, now - ANSWERED_WITHIN);
    let pool = services.db.pool();
    let pending = unreplied_gateway_events(pool, active, since, now, PER_PASS).await?;
    // The threads an answer of this pass is still to be written in (to be
    // tried again, or its outcome not recorded): the later answers there
    // wait for it.
    let mut held = HashSet::new();
    let mut sent = 0;
    for event in pending {
        match reply(services, event, now, &mut held).await {
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

/// Answers `event` in its thread, unless an earlier answer there is
/// `held`; whether it was sent.
async fn reply(
    services: &Services,
    event: UnrepliedEvent,
    now: DateTime<Utc>,
    held: &mut HashSet<Uuid>,
) -> anyhow::Result<bool> {
    let gateways = &services.gateways;
    let mut tx = services.db.begin(event.tenant_id).await?;
    let Some(target) = gateway_reply_target(&mut tx, event.event_id).await? else {
        return Ok(false);
    };
    let (tenant_id, event_id, thread_id) = (event.tenant_id, target.event_id, target.thread_id);
    if held.contains(&thread_id) {
        return Ok(false);
    }
    if !target.gateway_enabled || !target.still_linked {
        let skipped = ReplyRecord::Done("skipped");
        record_gateway_reply(&mut tx, tenant_id, event_id, thread_id, skipped).await?;
        tx.commit().await?;
        return Ok(false);
    }
    // Held until its outcome is recorded: should anything fail on the way,
    // the thread's later answers wait for the next pass.
    held.insert(thread_id);
    let token = gateways
        .secret(&mut tx, tenant_id, &bot_token_name(target.gateway_id))
        .await?;
    // A pause: the actions still awaiting a decision, at the time it is
    // answered (ADR-129).
    let awaiting = if target.status == "awaiting_approval" {
        list_approvals_for_run(&mut tx, target.run_id)
            .await?
            .into_iter()
            .filter(|a| a.status == "pending")
            .collect()
    } else {
        Vec::new()
    };
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
            let (text, blocks) = if target.status == "awaiting_approval" {
                let own: Vec<_> = (awaiting.iter())
                    .filter(|a| !a.routed_to_manager())
                    .map(|a| (a.id, a.action_summary.as_str()))
                    .collect();
                let managed = awaiting.len() - own.len();
                pause_reply(&url, &Pause { own: &own, managed })
            } else {
                (
                    reply_text(&target.status, target.answer.as_deref(), &url),
                    None,
                )
            };
            let thread = Some(target.thread_ts.as_str());
            match gateways
                .slack()
                .post_blocks(&token, &target.channel_id, thread, &text, blocks.as_deref())
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
    if matches!(record, ReplyRecord::Done(_)) {
        held.remove(&thread_id);
    }
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
    // 30 s, 2 min, 8 min; or when Slack says, within an hour.
    let wait = match failure {
        SlackFailure::Throttled {
            retry_after: Some(after),
        } => Duration::from_std(*after).unwrap_or(MAX_WAIT),
        _ => Duration::seconds(30 * 4_i64.pow(attempt.saturating_sub(1) as u32)),
    };
    ReplyRecord::Retry(now + wait.min(MAX_WAIT))
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
        // Whatever Slack says, within an hour.
        let far = SlackFailure::Throttled {
            retry_after: Some(std::time::Duration::from_secs(100_000_000_000_000)),
        };
        assert_eq!(
            after_failure(&far, 1, now),
            ReplyRecord::Retry(now + Duration::hours(1))
        );
        let refused = SlackFailure::Refused("channel_not_found".into());
        assert_eq!(
            after_failure(&refused, 1, now),
            ReplyRecord::Done("refused")
        );
    }
}
