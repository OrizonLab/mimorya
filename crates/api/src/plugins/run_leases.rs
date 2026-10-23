//! Execution leases in the background (ADR-083): the runs executing here
//! renew theirs every `renew_every`, until the service has stopped and its
//! runs ended; once the runs a stop interrupted are taken up at startup,
//! the runs whose lease expired are taken up every `take_up_every`, until
//! the service stops.

use chrono::Utc;
use tokio::task::JoinHandle;

use crate::app::AppState;

pub fn spawn_lease_renewal(services: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(services.runs.lease_timing().renew_every);
        loop {
            interval.tick().await;
            if let Err(error) = services.runs.renew_leases().await {
                tracing::warn!(error = format!("{error:#}"), "Run leases not renewed");
            }
        }
    })
}

pub fn spawn_lease_takeover(services: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut stopping = services.shutdown.subscribe();
        tokio::select! {
            () = services.runs.interrupted_runs_taken_up() => {}
            _ = stopping.wait_for(|stop| *stop) => return,
        }
        let mut interval = tokio::time::interval(services.runs.lease_timing().take_up_every);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = stopping.wait_for(|stop| *stop) => return,
            }
            match services.runs.take_up_expired(Utc::now()).await {
                Ok(0) => {}
                Ok(taken) => tracing::warn!(taken, "Runs with an expired lease taken up"),
                Err(error) => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "Expired run leases not taken up"
                    );
                }
            }
        }
    })
}
