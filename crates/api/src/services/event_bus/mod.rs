//! Fan-out of run events to the SSE subscribers of this process, and to the
//! other processes through PostgreSQL (ADR-083). An event is published here
//! as soon as it is recorded; the other processes learn of it by a notice
//! (`notice.rs`) that their relay (`relay.rs`) turns into the same event:
//! read from the run's journal when persisted, carried by the notice when
//! transient (text deltas, sent in order by `outbox.rs`). Each process
//! ignores its own notices.
//!
//! A subscriber that cannot keep up is dropped rather than buffered without
//! bound: its stream ends and the client reconnects with `Last-Event-ID`,
//! replaying the persisted events it missed.

mod notice;
mod outbox;
mod relay;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

pub use notice::Notice;
pub use outbox::{Delta, Outbox};
pub use relay::Relay;

const SUBSCRIBER_BUFFER: usize = 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct BusEvent {
    /// Persisted sequence number; `None` for transient events (text deltas).
    pub seq: Option<i32>,
    pub event_type: String,
    pub data: Value,
}

/// A run's subscribers, with its company.
struct Followers {
    tenant_id: Uuid,
    subscribers: Vec<(u64, mpsc::Sender<BusEvent>)>,
}

pub struct RunEventBus {
    /// This process, as its notices name it.
    origin: Uuid,
    listeners: Mutex<HashMap<Uuid, Followers>>,
    next_id: AtomicU64,
    /// Text deltas on their way to the other processes.
    outgoing: mpsc::Sender<Delta>,
}

impl Default for RunEventBus {
    /// A bus whose deltas reach no other process.
    fn default() -> Self {
        Self::new().0
    }
}

impl RunEventBus {
    /// The bus, and the deltas it hands over to the other processes.
    pub fn new() -> (Self, Outbox) {
        let (outgoing, deltas) = mpsc::channel(outbox::CAPACITY);
        let bus = Self {
            origin: Uuid::new_v4(),
            listeners: Mutex::default(),
            next_id: AtomicU64::new(0),
            outgoing,
        };
        (bus, Outbox::new(deltas))
    }

    /// This process, as its notices name it.
    pub fn origin(&self) -> Uuid {
        self.origin
    }

    /// Hands `event` to this process's subscribers of `run_id`.
    pub fn publish(&self, run_id: Uuid, event: BusEvent) {
        let mut listeners = self.listeners.lock().expect("event bus");
        let Some(followers) = listeners.get_mut(&run_id) else {
            return;
        };
        followers
            .subscribers
            .retain(|(_, sender)| sender.try_send(event.clone()).is_ok());
        if followers.subscribers.is_empty() {
            listeners.remove(&run_id);
        }
    }

    /// Hands a text delta to this process's subscribers, then to the other
    /// processes' (best effort: dropped when too many wait). `after` is the
    /// last event the run recorded, if known: a delta older than an event
    /// a process already relayed is stale there.
    pub fn publish_delta(&self, run_id: Uuid, after: Option<i32>, text: &str) {
        self.publish(run_id, notice::delta_event(text));
        let delta = Delta {
            run_id,
            after,
            text: text.into(),
        };
        // Full: the other processes miss part of a draft its message replaces.
        let _ = self.outgoing.try_send(delta);
    }

    pub fn subscribe(self: &Arc<Self>, tenant_id: Uuid, run_id: Uuid) -> Subscription {
        let (sender, receiver) = mpsc::channel(SUBSCRIBER_BUFFER);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.listeners
            .lock()
            .expect("event bus")
            .entry(run_id)
            .or_insert_with(|| Followers {
                tenant_id,
                subscribers: Vec::new(),
            })
            .subscribers
            .push((id, sender));
        Subscription {
            bus: self.clone(),
            run_id,
            id,
            receiver,
        }
    }

    /// Whether a subscriber of this process follows `run_id`.
    pub fn follows(&self, run_id: Uuid) -> bool {
        self.listeners
            .lock()
            .expect("event bus")
            .contains_key(&run_id)
    }

    /// The company of whom follows `run_id` here, if anybody does: the one
    /// whose event stream checked the run is theirs.
    pub fn follower_company(&self, run_id: Uuid) -> Option<Uuid> {
        let listeners = self.listeners.lock().expect("event bus");
        listeners.get(&run_id).map(|followers| followers.tenant_id)
    }

    /// The runs this process's subscribers follow, with their company.
    pub fn followed(&self) -> Vec<(Uuid, Uuid)> {
        self.listeners
            .lock()
            .expect("event bus")
            .iter()
            .map(|(run, followers)| (followers.tenant_id, *run))
            .collect()
    }

    fn unsubscribe(&self, run_id: Uuid, id: u64) {
        let mut listeners = self.listeners.lock().expect("event bus");
        if let Some(followers) = listeners.get_mut(&run_id) {
            followers.subscribers.retain(|(other, _)| *other != id);
            if followers.subscribers.is_empty() {
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
        let (tenant, run) = (Uuid::new_v4(), Uuid::new_v4());
        let mut subscription = bus.subscribe(tenant, run);
        assert!(bus.follows(run));
        assert_eq!(bus.follower_company(run), Some(tenant));
        assert_eq!(bus.followed(), [(tenant, run)]);
        bus.publish(Uuid::new_v4(), event(9));
        bus.publish(run, event(1));
        assert_eq!(subscription.recv().await.unwrap().seq, Some(1));
        assert!(subscription.is_empty());
        drop(subscription);
        assert!(!bus.follows(run));
        assert!(bus.listeners.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_delta_goes_to_subscribers_here_and_to_the_other_processes() {
        let (bus, mut outbox) = RunEventBus::new();
        let bus = Arc::new(bus);
        let (tenant, run) = (Uuid::new_v4(), Uuid::new_v4());
        let mut subscription = bus.subscribe(tenant, run);
        bus.publish_delta(run, Some(4), "Bonjour");
        let here = subscription.recv().await.unwrap();
        assert_eq!(
            (here.seq, here.event_type.as_str()),
            (None, "assistant_delta")
        );
        assert_eq!(here.data, json!({ "text": "Bonjour" }));
        let sent = outbox.next_batch().await.unwrap();
        assert_eq!(
            (sent.run_id, sent.after, sent.text.as_str()),
            (run, Some(4), "Bonjour")
        );
    }
}
