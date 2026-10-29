//! Answering in Slack the runs people started there (ADR-127), and telling
//! there the managers whom requests await (ADR-130), every few seconds, in
//! each process that executes runs: one at a time answers them, one at a
//! time tells them (ADR-086), each end or pause, each request, once. Each
//! in its own loop: a slow Slack telling managers holds no conversation's
//! answer back. A process that has not started executing runs does
//! neither.

use std::future::Future;
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::app::AppState;
use crate::services::gateways::{notify_approvers_in_slack, reply_to_finished_runs};

/// How often the runs' ends and pauses are answered, and the managers told:
/// a conversation in Slack waits for its answer.
const PASS_INTERVAL: Duration = Duration::from_secs(3);

pub fn spawn_gateway_replies(services: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let answers = every_pass(services.clone(), "Slack answers", |services| async move {
            reply_to_finished_runs(&services).await
        });
        let notices = every_pass(services, "Slack approval notices", |services| async move {
            notify_approvers_in_slack(&services).await
        });
        tokio::join!(answers, notices);
    })
}

/// Runs `pass` every `PASS_INTERVAL` until the service stops.
async fn every_pass<F, P>(services: AppState, name: &'static str, pass: P)
where
    P: Fn(AppState) -> F,
    F: Future<Output = anyhow::Result<usize>>,
{
    let mut stopping = services.shutdown.subscribe();
    let mut interval = tokio::time::interval(PASS_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = stopping.wait_for(|stop| *stop) => return,
        }
        match pass(services.clone()).await {
            Ok(0) => {}
            Ok(sent) => tracing::debug!(sent, pass = name, "Slack messages sent"),
            Err(error) => tracing::warn!(error = %error, pass = name, "Slack pass failed"),
        }
    }
}
