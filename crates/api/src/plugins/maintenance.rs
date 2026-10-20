//! Periodic maintenance, at startup and then every minute: expired password
//! reset links are deleted (ADR-028); runs without progress for a while
//! belong to a stuck process and are marked as failed, their `run_finished`
//! event recorded and broadcast to the clients following them. That sweep
//! waits until the process has run as long as a run takes to go stale: runs
//! older than the process are the ones a stop of the service interrupted,
//! taken up at startup instead (ADR-053), and a run taken up keeps its last
//! saved time until it saves its next step. A stuck process that wakes up later finds its run finished: it
//! announces nothing more (`outcome::record_crash`), though a step it was
//! taking may still append an event after that end, which clients ignore
//! (they stop at the first `run_finished`).

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use mimorya_agent::runtime::RunEvent;
use mimorya_db::Database;
use mimorya_db::repositories::password_resets::purge_expired_password_resets;
use mimorya_db::repositories::runs::fail_stale_runs;
use tokio::task::JoinHandle;

use crate::services::event_bus::RunEventBus;
use crate::services::runs::record_run_event;

const STALE_AFTER: chrono::Duration = chrono::Duration::minutes(15);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn_maintenance(db: Database, bus: Arc<RunEventBus>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        let started = tokio::time::Instant::now();
        loop {
            interval.tick().await;
            if sweeps_stale_runs(started.elapsed()) {
                match sweep_stale_runs(&db, &bus, STALE_AFTER).await {
                    Ok(0) => {}
                    Ok(failed) => tracing::warn!(failed, "Stale runs marked as failed"),
                    Err(error) => tracing::warn!(error = %error, "Stale run sweep failed"),
                }
            }
            if let Err(error) = purge_expired_password_resets(db.pool()).await {
                tracing::warn!(error = %error, "Expired reset link purge failed");
            }
        }
    })
}

/// Whether a process up for `uptime` sweeps stale runs: once it has run as
/// long as a run takes to go stale, so that only its own runs can be.
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
    fn sweeps_stale_runs_only_once_up_as_long_as_a_run_takes_to_go_stale() {
        assert!(!sweeps_stale_runs(Duration::ZERO));
        assert!(!sweeps_stale_runs(Duration::from_secs(14 * 60 + 59)));
        assert!(sweeps_stale_runs(Duration::from_secs(15 * 60)));
    }
}
