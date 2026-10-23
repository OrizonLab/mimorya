//! Firing the scheduled tasks due (ADR-080), every minute, in the process
//! that executes runs only (it holds the run executor's lock, ADR-053): a
//! process that does not, about to refuse to start, fires nothing.

use std::time::Duration;

use chrono::Utc;
use tokio::task::JoinHandle;

use crate::app::AppState;
use crate::services::scheduled_tasks::fire_due_tasks;

const PASS_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn_scheduled_tasks(services: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(PASS_INTERVAL);
        loop {
            interval.tick().await;
            if !services.runs.executes().await {
                continue;
            }
            match fire_due_tasks(&services, Utc::now()).await {
                Ok(0) => {}
                Ok(started) => tracing::info!(started, "Scheduled tasks fired"),
                Err(error) => tracing::warn!(error = %error, "Scheduled task pass failed"),
            }
        }
    })
}
