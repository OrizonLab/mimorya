//! Run events across processes in the background (ADR-084): the other
//! processes' events relayed to this one's subscribers until the service
//! stops, in a process that serves the HTTP API; this one's text deltas sent
//! to the others until its runs have ended, in one that executes runs
//! (ADR-085).

use tokio::task::JoinHandle;

use crate::app::AppState;
use crate::services::event_bus::{Outbox, Relay};

pub fn spawn_run_event_relay(services: AppState) -> JoinHandle<()> {
    let relay = Relay::new(services.bus.clone(), services.db.clone());
    let url = services.config.database_app_url.clone();
    tokio::spawn(relay.listen(url, services.shutdown.subscribe()))
}

/// Sends this process's text deltas; `listens` says whether it relays the
/// others' events too.
pub fn spawn_delta_outbox(services: &AppState, outbox: Outbox, listens: bool) -> JoinHandle<()> {
    let (db, origin) = (services.db.clone(), services.bus.origin());
    tokio::spawn(outbox.send_all(db, origin, listens))
}
