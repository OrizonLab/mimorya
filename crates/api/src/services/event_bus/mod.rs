//! Fan-out of run events to the SSE subscribers of this process, and to the
//! other processes through PostgreSQL (ADR-084). An event is published here
//! as soon as it is recorded; the other processes learn of it by a notice
//! (`notice.rs`) that their relay (`relay.rs`) turns into the same event:
//! read from the run's journal when persisted, carried by the notice when
//! transient (text deltas, sent in order by `outbox.rs`). Each process
//! ignores its own notices. What the relay knows of a run (the last event
//! handed to its subscribers) lives with them, and goes with the last.
//!
//! A subscriber that cannot keep up is dropped rather than buffered without
//! bound: its stream ends and the client reconnects with `Last-Event-ID`,
//! replaying the persisted events it missed.

mod listener;
mod notice;
mod outbox;
mod relay;
mod subscription;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

pub use notice::Notice;
pub use outbox::{Delta, Outbox};
pub use relay::Relay;
pub use subscription::Subscription;

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
    /// The last recorded event they all have: replayed by their streams,
    /// or published since. The relay reads the run's journal from there.
    handed: Option<i32>,
    subscribers: Vec<(u64, mpsc::Sender<BusEvent>)>,
}

/// What this process's subscribers of a run have, as the relay reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Followed {
    pub tenant_id: Uuid,
    pub run_id: Uuid,
    pub handed: Option<i32>,
}

pub struct RunEventBus {
    /// This process, as its notices name it.
    origin: Uuid,
    listeners: Mutex<HashMap<Uuid, Followers>>,
    next_id: AtomicU64,
    /// Text deltas on their way to the other processes.
    outgoing: mpsc::Sender<Delta>,
    /// Whether the relay listens to the other processes.
    relaying: AtomicBool,
    /// Whether a read of the relay failed, to be tried again.
    relay_behind: AtomicBool,
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
            relaying: AtomicBool::new(false),
            relay_behind: AtomicBool::new(false),
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
        if let Some(seq) = event.seq {
            followers.handed = followers.handed.max(Some(seq));
        }
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

    /// Subscribes to `run_id`'s events, the run being `tenant_id`'s: checked
    /// by the caller, as the relay reads the run's journal in that company.
    pub fn subscribe(self: &Arc<Self>, tenant_id: Uuid, run_id: Uuid) -> Subscription {
        let (sender, receiver) = mpsc::channel(SUBSCRIBER_BUFFER);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut listeners = self.listeners.lock().expect("event bus");
        let followers = listeners.entry(run_id).or_insert_with(|| Followers {
            tenant_id,
            handed: None,
            subscribers: Vec::new(),
        });
        debug_assert_eq!(followers.tenant_id, tenant_id, "a run of one company");
        followers.subscribers.push((id, sender));
        drop(listeners);
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

    /// What this process's subscribers of `run_id` have, if any follows it,
    /// in the company whose event stream checked the run is theirs.
    pub fn followed_run(&self, run_id: Uuid) -> Option<Followed> {
        let listeners = self.listeners.lock().expect("event bus");
        listeners
            .get(&run_id)
            .map(|followers| followed(run_id, followers))
    }

    /// Every run this process's subscribers follow.
    pub fn followed(&self) -> Vec<Followed> {
        let listeners = self.listeners.lock().expect("event bus");
        listeners
            .iter()
            .map(|(run_id, followers)| followed(*run_id, followers))
            .collect()
    }

    /// Whether the relay listens to the other processes.
    pub fn relaying(&self) -> bool {
        self.relaying.load(Ordering::Relaxed)
    }

    pub(super) fn set_relaying(&self, relaying: bool) {
        self.relaying.store(relaying, Ordering::Relaxed);
    }

    /// Whether a read of the relay failed, to be tried again.
    pub fn relay_behind(&self) -> bool {
        self.relay_behind.load(Ordering::Relaxed)
    }

    pub(super) fn set_relay_behind(&self, behind: bool) {
        self.relay_behind.store(behind, Ordering::Relaxed);
    }

    /// A subscriber of `run_id` replayed its journal up to `seq`: the relay
    /// reads it from there at most (from earlier, should another subscriber
    /// have replayed less).
    pub(super) fn replayed(&self, run_id: Uuid, seq: i32) {
        let mut listeners = self.listeners.lock().expect("event bus");
        if let Some(followers) = listeners.get_mut(&run_id) {
            followers.handed = Some(followers.handed.map_or(seq, |handed| handed.min(seq)));
        }
    }

    pub(super) fn unsubscribe(&self, run_id: Uuid, id: u64) {
        let mut listeners = self.listeners.lock().expect("event bus");
        if let Some(followers) = listeners.get_mut(&run_id) {
            followers.subscribers.retain(|(other, _)| *other != id);
            if followers.subscribers.is_empty() {
                listeners.remove(&run_id);
            }
        }
    }
}

fn followed(run_id: Uuid, followers: &Followers) -> Followed {
    Followed {
        tenant_id: followers.tenant_id,
        run_id,
        handed: followers.handed,
    }
}

#[cfg(test)]
mod tests;
