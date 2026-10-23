//! Firing the scheduled tasks due (ADR-080), every minute, in the process
//! that executes runs only, once it has taken up the runs a stop interrupted
//! (ADR-053): a run started meanwhile would be taken for one of them. A
//! process that does not hold the run executor's lock, about to refuse to
//! start, fires nothing. On shutdown, the pass under way stops between two
//! tasks: the one being fired is not cut short.

use std::time::Duration;

use chrono::Utc;
use tokio::task::JoinHandle;

use crate::app::AppState;
use crate::services::scheduled_tasks::fire_due_tasks;

const PASS_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn_scheduled_tasks(services: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut stopping = services.shutdown.subscribe();
        tokio::select! {
            () = services.runs.interrupted_runs_taken_up() => {}
            _ = stopping.wait_for(|stop| *stop) => return,
        }
        let mut interval = tokio::time::interval(PASS_INTERVAL);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = stopping.wait_for(|stop| *stop) => return,
            }
            match fire_due_tasks(&services, Utc::now()).await {
                Ok(0) => {}
                Ok(started) => tracing::info!(started, "Scheduled tasks fired"),
                Err(error) => tracing::warn!(error = %error, "Scheduled task pass failed"),
            }
        }
    })
}
