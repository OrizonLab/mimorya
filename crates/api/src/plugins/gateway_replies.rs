//! Answering in Slack the runs people started there (ADR-127), every few
//! seconds, in each process that executes runs: one at a time answers them
//! (ADR-086), each end or pause once. A process that has not started
//! executing runs answers nothing.

use std::time::Duration;

use tokio::task::JoinHandle;

use crate::app::AppState;
use crate::services::gateways::reply_to_finished_runs;

/// How often the runs' ends and pauses are answered: a conversation in
/// Slack waits for its answer.
const PASS_INTERVAL: Duration = Duration::from_secs(3);

pub fn spawn_gateway_replies(services: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut stopping = services.shutdown.subscribe();
        let mut interval = tokio::time::interval(PASS_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = stopping.wait_for(|stop| *stop) => return,
            }
            match reply_to_finished_runs(&services).await {
                Ok(0) => {}
                Ok(sent) => tracing::debug!(sent, "Slack answers sent"),
                Err(error) => tracing::warn!(error = %error, "Slack answer pass failed"),
            }
        }
    })
}
