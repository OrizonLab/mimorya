//! What a process runs in the background, by its role (ADR-085): the
//! sharing of cache invalidations in every process; the relay of run
//! events where HTTP is served; the sweeps, passes and run queue where runs
//! are executed. Stopped in an order that lets runs end first.

use tokio::task::JoinHandle;

use super::AppState;
use crate::plugins::document_labels::spawn_document_labels;
use crate::plugins::gateway_replies::spawn_gateway_replies;
use crate::plugins::maintenance::spawn_maintenance;
use crate::plugins::memory_indexing::spawn_memory_indexing;
use crate::plugins::run_event_relay::{spawn_delta_outbox, spawn_run_event_relay};
use crate::plugins::run_leases::{spawn_lease_renewal, spawn_lease_takeover};
use crate::plugins::run_queue::spawn_run_queue;
use crate::plugins::scheduled_tasks::spawn_scheduled_tasks;
use crate::services::event_bus::Outbox;
use crate::services::invalidations::{Outgoing, share_invalidations};

/// What the tests leave to be done on demand (`BuildOptions`).
pub(super) struct OnDemand {
    pub indexing: bool,
    pub document_labels: bool,
    pub scheduling: bool,
    pub gateway_replies: bool,
}

#[derive(Default)]
pub(super) struct Background {
    maintenance: Option<JoinHandle<()>>,
    indexing: Option<JoinHandle<()>>,
    document_labels: Option<JoinHandle<()>>,
    scheduling: Option<JoinHandle<()>>,
    gateway_replies: Option<JoinHandle<()>>,
    lease_renewal: Option<JoinHandle<()>>,
    lease_takeover: Option<JoinHandle<()>>,
    run_queue: Option<JoinHandle<()>>,
    event_relay: Option<JoinHandle<()>>,
    delta_outbox: Option<JoinHandle<()>>,
    invalidations: Option<JoinHandle<()>>,
}

impl Background {
    /// Starts what the process's role runs: none of it but the relay in an
    /// `api` process, which executes no runs; no relay in a `worker`, which
    /// serves no event stream.
    pub(super) fn start(
        services: &AppState,
        on_demand: OnDemand,
        outgoing: Outgoing,
        outbox: Outbox,
    ) -> Self {
        let role = services.config.role;
        let mut background = Background {
            invalidations: Some(tokio::spawn(share_invalidations(
                services.clone(),
                outgoing,
            ))),
            ..Background::default()
        };
        if role.serves_http() {
            background.event_relay = Some(spawn_run_event_relay(services.clone()));
        }
        if role.executes_runs() {
            background.maintenance = Some(spawn_maintenance(services.clone()));
            background.indexing =
                (!on_demand.indexing).then(|| spawn_memory_indexing(services.clone()));
            background.document_labels =
                (!on_demand.document_labels).then(|| spawn_document_labels(services.clone()));
            background.scheduling =
                (!on_demand.scheduling).then(|| spawn_scheduled_tasks(services.clone()));
            background.gateway_replies =
                (!on_demand.gateway_replies).then(|| spawn_gateway_replies(services.clone()));
            background.lease_renewal = Some(spawn_lease_renewal(services.clone()));
            background.lease_takeover = Some(spawn_lease_takeover(services.clone()));
            background.run_queue = Some(spawn_run_queue(services.clone()));
            let listens = role.serves_http();
            background.delta_outbox = Some(spawn_delta_outbox(services, outbox, listens));
        }
        background
    }

    /// Stops it, the runs and requests' background work awaited in between.
    pub(super) async fn stop(self, services: &AppState) {
        // The streams it fed end now.
        abort(self.event_relay).await;
        // Ended before the runs are awaited: its expiry sweep may start one.
        abort(self.maintenance).await;
        // So is this one, which starts runs: it stops between two tasks,
        // never in the middle of firing one.
        wait(self.scheduling).await;
        // Nor do these: they take up or claim no other run once the
        // service stops.
        wait(self.lease_takeover).await;
        wait(self.run_queue).await;
        abort(self.indexing).await;
        // A document being classified keeps its request: asked again by the
        // next process that starts.
        abort(self.document_labels).await;
        // A Slack message being answered may start a run as it ends: awaited
        // first, so that its run is awaited with the others.
        services.background.idle().await;
        services.runs.idle().await;
        // The runs ended: what remains to answer in Slack is answered by
        // the next process that starts.
        abort(self.gateway_replies).await;
        // The runs ended: no lease left to renew, nor text to send.
        abort(self.lease_renewal).await;
        abort(self.delta_outbox).await;
        services.background.idle().await;
        // Nothing left to change in the caches.
        abort(self.invalidations).await;
    }
}

/// Stops `task`, if any, at once.
async fn abort(task: Option<JoinHandle<()>>) {
    if let Some(task) = task {
        task.abort();
        let _ = task.await;
    }
}

/// Waits for `task`, if any, to stop by itself.
async fn wait(task: Option<JoinHandle<()>>) {
    if let Some(task) = task {
        let _ = task.await;
    }
}
