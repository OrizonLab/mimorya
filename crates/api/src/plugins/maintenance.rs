//! Periodic maintenance, at startup and then every minute: runs without
//! progress for a while belong to a stuck process and are marked as failed,
//! their `run_finished` event recorded and broadcast to the clients
//! following them (from the second pass on: runs a stop of the service left
//! running are taken up at startup first, ADR-053); expired password reset
//! links are deleted (ADR-028). A stuck process that wakes up later finds its run finished: it
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
        let mut at_startup = true;
        loop {
            interval.tick().await;
            if !std::mem::take(&mut at_startup) {
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
