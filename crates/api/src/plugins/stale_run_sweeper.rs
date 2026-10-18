//! Runs without progress for a while belong to a crashed or stuck process:
//! they are marked as failed at startup and then every minute.

use std::time::Duration;

use chrono::Utc;
use mimorya_db::Database;
use mimorya_db::repositories::runs::fail_stale_runs;
use tokio::task::JoinHandle;

const STALE_AFTER: chrono::Duration = chrono::Duration::minutes(15);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn_stale_run_sweeper(db: Database) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            interval.tick().await;
            match fail_stale_runs(db.pool(), Utc::now() - STALE_AFTER).await {
                Ok(0) => {}
                Ok(failed) => tracing::warn!(failed, "Stale runs marked as failed"),
                Err(error) => tracing::warn!(error = %error, "Stale run sweep failed"),
            }
        }
    })
}
