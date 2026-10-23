//! A subscriber's events of one run (ADR-084).

use std::sync::Arc;

use tokio::sync::mpsc;
use uuid::Uuid;

use super::{BusEvent, RunEventBus};

/// Events of one run, in publication order. Unsubscribes when dropped.
pub struct Subscription {
    pub(super) bus: Arc<RunEventBus>,
    pub(super) run_id: Uuid,
    pub(super) id: u64,
    pub(super) receiver: mpsc::Receiver<BusEvent>,
}

impl Subscription {
    /// Its stream replayed the run's journal up to `seq` (ADR-084).
    pub fn replayed_through(&self, seq: i32) {
        self.bus.replayed(self.run_id, seq);
    }

    /// `None` once the bus dropped this subscriber.
    pub async fn recv(&mut self) -> Option<BusEvent> {
        self.receiver.recv().await
    }

    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.bus.unsubscribe(self.run_id, self.id);
    }
}
