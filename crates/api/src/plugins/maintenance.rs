//! Periodic maintenance, at startup and then every minute: runs without
//! progress for a while belong to a crashed or stuck process and are marked
//! as failed, their `run_finished` event recorded and broadcast to the
//! clients following them; expired password reset links are deleted
//! (ADR-028).

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use mimorya_agent::runtime::RunEvent;
use mimorya_db::Database;
use mimorya_db::repositories::password_resets::purge_expired_password_resets;
use mimorya_db::repositories::run_events::append_run_event;
use mimorya_db::repositories::runs::{StaleRun, fail_stale_runs};
use tokio::task::JoinHandle;

use crate::services::event_bus::{BusEvent, RunEventBus};

const STALE_AFTER: chrono::Duration = chrono::Duration::minutes(15);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn_maintenance(db: Database, bus: Arc<RunEventBus>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            interval.tick().await;
            match sweep_stale_runs(&db, &bus, STALE_AFTER).await {
                Ok(0) => {}
                Ok(failed) => tracing::warn!(failed, "Stale runs marked as failed"),
                Err(error) => tracing::warn!(error = %error, "Stale run sweep failed"),
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
        if let Err(error) = announce_end(db, bus, run).await {
            tracing::warn!(run_id = %run.run_id, error = format!("{error:#}"), "Stale run end not announced");
        }
    }
    Ok(failed.len())
}

async fn announce_end(db: &Database, bus: &RunEventBus, run: &StaleRun) -> anyhow::Result<()> {
    let (event_type, data) = RunEvent::RunFinished {
        status: "failed".into(),
        error: Some(run.error.clone()),
    }
    .into_parts();
    let mut tx = db.begin(run.tenant_id).await?;
    let row = append_run_event(&mut tx, run.tenant_id, run.run_id, &event_type, &data).await?;
    tx.commit().await?;
    bus.publish(
        run.run_id,
        BusEvent {
            seq: Some(row.seq),
            event_type,
            data,
        },
    );
    Ok(())
}
