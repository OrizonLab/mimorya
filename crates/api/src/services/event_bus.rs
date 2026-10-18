//! In-process fan-out of run events to SSE subscribers. Single instance only;
//! multi-instance deployments will relay through PostgreSQL LISTEN/NOTIFY
//! (see docs/ARCHITECTURE.md §8).
//!
//! A subscriber that cannot keep up is dropped rather than buffered without
//! bound: its stream ends and the client reconnects with `Last-Event-ID`,
//! replaying the persisted events it missed.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

const SUBSCRIBER_BUFFER: usize = 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct BusEvent {
    /// Persisted sequence number; `None` for transient events (text deltas).
    pub seq: Option<i32>,
    pub event_type: String,
    pub data: Value,
}

type Listeners = HashMap<Uuid, Vec<(u64, mpsc::Sender<BusEvent>)>>;

#[derive(Default)]
pub struct RunEventBus {
    listeners: Mutex<Listeners>,
    next_id: AtomicU64,
}

impl RunEventBus {
    pub fn publish(&self, run_id: Uuid, event: BusEvent) {
        let mut listeners = self.listeners.lock().expect("event bus");
        let Some(subscribers) = listeners.get_mut(&run_id) else {
            return;
        };
        subscribers.retain(|(_, sender)| sender.try_send(event.clone()).is_ok());
        if subscribers.is_empty() {
            listeners.remove(&run_id);
        }
    }

    pub fn subscribe(self: &Arc<Self>, run_id: Uuid) -> Subscription {
        let (sender, receiver) = mpsc::channel(SUBSCRIBER_BUFFER);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.listeners
            .lock()
            .expect("event bus")
            .entry(run_id)
            .or_default()
            .push((id, sender));
        Subscription {
            bus: self.clone(),
            run_id,
            id,
            receiver,
        }
    }

    fn unsubscribe(&self, run_id: Uuid, id: u64) {
        let mut listeners = self.listeners.lock().expect("event bus");
        if let Some(subscribers) = listeners.get_mut(&run_id) {
            subscribers.retain(|(other, _)| *other != id);
            if subscribers.is_empty() {
                listeners.remove(&run_id);
            }
        }
    }
}

/// Events of one run, in publication order. Unsubscribes when dropped.
pub struct Subscription {
    bus: Arc<RunEventBus>,
    run_id: Uuid,
    id: u64,
    receiver: mpsc::Receiver<BusEvent>,
}

impl Subscription {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn event(seq: i32) -> BusEvent {
        BusEvent {
            seq: Some(seq),
            event_type: "tool_call".into(),
            data: json!({}),
        }
    }

    #[tokio::test]
    async fn delivers_events_of_the_subscribed_run_only() {
        let bus = Arc::new(RunEventBus::default());
        let run = Uuid::new_v4();
        let mut subscription = bus.subscribe(run);
        bus.publish(Uuid::new_v4(), event(9));
        bus.publish(run, event(1));
        assert_eq!(subscription.recv().await.unwrap().seq, Some(1));
        assert!(subscription.is_empty());
        drop(subscription);
        assert!(bus.listeners.lock().unwrap().is_empty());
    }
}
