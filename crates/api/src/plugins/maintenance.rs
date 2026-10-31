//! Periodic maintenance, at startup and then every minute, in each process
//! that executes runs: expired password reset links are deleted (ADR-028),
//! ended absences (ADR-071), the shared rate limit windows long over
//! (ADR-088), the evaluations of skills ended ninety days ago (ADR-104),
//! the events a gateway took a day ago (ADR-125), the Telegram link codes
//! expired (ADR-133) and the cards of Teams notices to managers settled, or
//! no longer where their manager is written (ADR-142); from the second
//! minute, the notices a
//! stop of the service lost are sent (ADR-070), requests nobody decided in
//! time expire, and their runs go on (ADR-061), their managers reminded the
//! day before (ADR-062), each of these three in one process at a time
//! (ADR-086); the authors of the missions over are told of their end, and
//! the skills' administrators of the agent's proposals, each in one process
//! at a time too (ADR-091, ADR-102); the evaluations of the skills asked for
//! are played, in the background and one at a time (ADR-104); runs without
//! progress for a while belong to a stuck process and are marked as
//! failed, their `run_finished` event recorded and broadcast to the clients
//! following them. That sweep waits until the process has run as long as a
//! run takes to go stale: a run whose executor stopped is taken up by its
//! lease instead (ADR-083), which counts as its progress. A stuck process
//! that wakes up later finds its run finished: it
//! announces nothing more (`outcome::record_crash`), though a step it was
//! taking may still append an event after that end, which clients ignore
//! (they stop at the first `run_finished`).

use std::time::Duration;

use chrono::Utc;
use mimorya_agent::runtime::RunEvent;
use mimorya_core::skill_evaluations::EVALUATION_KEPT_DAYS;
use mimorya_db::Database;
use mimorya_db::repositories::approval_delegations::purge_ended_approval_delegations;
use mimorya_db::repositories::gateway_cards::purge_notice_cards;
use mimorya_db::repositories::gateway_link_codes::purge_link_codes;
use mimorya_db::repositories::gateways::purge_gateway_events;
use mimorya_db::repositories::password_resets::purge_expired_password_resets;
use mimorya_db::repositories::rate_limits::purge_rate_limits;
use mimorya_db::repositories::runs::fail_stale_runs;
use mimorya_db::repositories::skill_evaluations::purge_finished_evaluations;
use tokio::task::JoinHandle;

use crate::app::AppState;
use crate::services::event_bus::RunEventBus;
use crate::services::missions::tell_ended_missions;
use crate::services::runs::record_run_event;
use crate::services::skill_evaluations::wake as play_skill_evaluations;
use crate::services::skill_proposal_notice::announce_skill_proposals;

const STALE_AFTER: chrono::Duration = chrono::Duration::minutes(15);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
/// Past the longest rate limit window (an hour, ADR-088), a window counts
/// for nothing more.
const RATE_LIMITS_KEPT: chrono::Duration = chrono::Duration::hours(2);
/// Slack sends an event again for a few hours at most: a day past it, an
/// event's identifier serves nothing more (ADR-125).
const GATEWAY_EVENTS_KEPT: chrono::Duration = chrono::Duration::days(1);
/// How long the card of a notice to a manager in Teams, or its message in
/// Telegram, is kept: its requests, told of within a day, expire after a
/// week (ADR-142, ADR-148).
const NOTICE_CARDS_KEPT: chrono::Duration = chrono::Duration::days(8);

pub fn spawn_maintenance(services: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let (db, bus) = (&services.db, &services.bus);
        // Ticks at startup, then every `SWEEP_INTERVAL` from it: the uptimes
        // the sweeps wait for fall on ticks.
        let started = tokio::time::Instant::now();
        let mut interval = tokio::time::interval_at(started, SWEEP_INTERVAL);
        loop {
            interval.tick().await;
            if sweeps_requests(started.elapsed()) {
                sweep_requests(&services).await;
            }
            match tell_ended_missions(&services).await {
                Ok(0) => {}
                Ok(told) => tracing::info!(told, "Missions' authors told of their end"),
                Err(error) => tracing::warn!(error = %error, "Mission end notice sweep failed"),
            }
            match announce_skill_proposals(&services).await {
                Ok(0) => {}
                Ok(announced) => tracing::info!(announced, "Skill proposals announced"),
                Err(error) => tracing::warn!(error = %error, "Skill proposal notice sweep failed"),
            }
            // Played apart, for as long as they take (ADR-104).
            play_skill_evaluations(&services);
            if sweeps_stale_runs(started.elapsed()) {
                match sweep_stale_runs(db, bus, STALE_AFTER).await {
                    Ok(0) => {}
                    Ok(failed) => tracing::warn!(failed, "Stale runs marked as failed"),
                    Err(error) => tracing::warn!(error = %error, "Stale run sweep failed"),
                }
            }
            if let Err(error) = purge_expired_password_resets(db.pool()).await {
                tracing::warn!(error = %error, "Expired reset link purge failed");
            }
            if let Err(error) = purge_ended_approval_delegations(db.pool()).await {
                tracing::warn!(error = %error, "Ended absence purge failed");
            }
            let over = Utc::now() - RATE_LIMITS_KEPT;
            if let Err(error) = purge_rate_limits(db.pool(), over).await {
                tracing::warn!(error = %error, "Rate limit window purge failed");
            }
            let ended = Utc::now() - chrono::Duration::days(EVALUATION_KEPT_DAYS);
            if let Err(error) = purge_finished_evaluations(db.pool(), ended).await {
                tracing::warn!(error = %error, "Skill evaluation purge failed");
            }
            let taken = Utc::now() - GATEWAY_EVENTS_KEPT;
            if let Err(error) = purge_gateway_events(db.pool(), taken).await {
                tracing::warn!(error = %error, "Gateway event purge failed");
            }
            // Settled, or no longer where their manager is written.
            let written = Utc::now() - NOTICE_CARDS_KEPT;
            if let Err(error) = purge_notice_cards(db.pool(), written).await {
                tracing::warn!(error = %error, "Notice card purge failed");
            }
            // Expired, a Telegram link code and whom it was sent back from
            // are forgotten (ADR-133).
            if let Err(error) = purge_link_codes(db.pool(), Utc::now()).await {
                tracing::warn!(error = %error, "Gateway link code purge failed");
            }
        }
    })
}

/// Whether a process up for `uptime` sweeps the requests: from the second
/// sweep, once it has taken up the runs whose executor stopped; the
/// workers already up sweep them meanwhile (ADR-086).
pub fn sweeps_requests(uptime: Duration) -> bool {
    uptime >= SWEEP_INTERVAL
}

/// Sends the notices a stop lost (ADR-070), expires the requests past their
/// deadline (ADR-061) and reminds managers of those due within a day
/// (ADR-062), in a process that executes runs.
async fn sweep_requests(services: &AppState) {
    match services.runs.catch_up_notices().await {
        Ok(0) => {}
        Ok(lost) => tracing::warn!(
            lost,
            "Managers and runs' authors told of requests a stop kept from them"
        ),
        Err(error) => tracing::warn!(error = %error, "Lost approval notice sweep failed"),
    }
    match services.runs.expire_overdue().await {
        Ok(0) => {}
        Ok(expired) => tracing::info!(expired, "Overdue approval requests expired"),
        Err(error) => tracing::warn!(error = %error, "Overdue request sweep failed"),
    }
    match services.runs.remind_due().await {
        Ok(0) => {}
        Ok(reminded) => tracing::info!(reminded, "Managers reminded of requests due"),
        Err(error) => tracing::warn!(error = %error, "Approval reminder sweep failed"),
    }
}

/// Whether a process up for `uptime` sweeps stale runs: once it has run as
/// long as a run takes to go stale, so that the runs a stop left running
/// are taken up by lease first, whichever worker starts (ADR-086).
pub fn sweeps_stale_runs(uptime: Duration) -> bool {
    STALE_AFTER
        .to_std()
        .is_ok_and(|stale_after| uptime >= stale_after)
}

/// Fails the runs without progress for `stale_after` and announces each end;
/// returns how many. A run whose announcement fails is still failed: its
/// clients learn it when they reload.
pub async fn sweep_stale_runs(
    db: &Database,
    bus: &RunEventBus,
    stale_after: chrono::Duration,
) -> anyhow::Result<usize> {
    let failed = fail_stale_runs(db.pool(), Utc::now() - stale_after).await?;
    for run in &failed {
        let end = RunEvent::RunFinished {
            status: "failed".into(),
            error: Some(run.error.clone()),
        };
        if let Err(error) = record_run_event(db, bus, run.tenant_id, run.run_id, end).await {
            tracing::warn!(run_id = %run.run_id, error = format!("{error:#}"), "Stale run end not announced");
        }
    }
    Ok(failed.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweeps_requests_from_the_second_sweep() {
        assert!(!sweeps_requests(Duration::ZERO));
        assert!(!sweeps_requests(Duration::from_secs(59)));
        assert!(sweeps_requests(Duration::from_secs(60)));
    }

    #[test]
    fn keeps_the_rate_limit_windows_past_the_longest() {
        use crate::plugins::rate_limit::{FAILED_LOGIN_WINDOW, RESET_REQUEST_WINDOW};
        let kept = RATE_LIMITS_KEPT.to_std().unwrap();
        // Logins and sign-in steps count by the minute.
        for window in [FAILED_LOGIN_WINDOW, RESET_REQUEST_WINDOW] {
            assert!(kept > window, "{window:?}");
        }
    }

    #[test]
    fn sweeps_stale_runs_only_once_up_as_long_as_a_run_takes_to_go_stale() {
        assert!(!sweeps_stale_runs(Duration::ZERO));
        assert!(!sweeps_stale_runs(Duration::from_secs(14 * 60 + 59)));
        assert!(sweeps_stale_runs(Duration::from_secs(15 * 60)));
    }
}
