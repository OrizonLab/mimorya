//! Firing the scheduled tasks due (ADR-080), every minute, in each process
//! that executes runs, once it has taken up the runs whose executor stopped
//! (ADR-053, ADR-086); each occurrence fires once, whichever process claims
//! it first. A process that has not started executing runs fires nothing.
//! On shutdown, the pass under way stops between two tasks: the one being
//! fired is not cut short.

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
