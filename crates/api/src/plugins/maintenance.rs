//! Periodic maintenance, at startup and then every minute: runs without
//! progress for a while belong to a crashed or stuck process and are marked
//! as failed; expired password reset links are deleted (ADR-028).

use std::time::Duration;

use chrono::Utc;
use mimorya_db::Database;
use mimorya_db::repositories::password_resets::purge_expired_password_resets;
use mimorya_db::repositories::runs::fail_stale_runs;
use tokio::task::JoinHandle;

const STALE_AFTER: chrono::Duration = chrono::Duration::minutes(15);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn_maintenance(db: Database) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        loop {
            interval.tick().await;
            match fail_stale_runs(db.pool(), Utc::now() - STALE_AFTER).await {
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
