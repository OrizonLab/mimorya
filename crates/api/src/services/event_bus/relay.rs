//! The other processes' run events, relayed to this process's subscribers
//! (ADR-083). A notice of a recorded event is read back from the run's
//! journal, in the company of whom follows the run here (row-level security
//! applies: the notice names no company), from the last event relayed for
//! the run: one lost in between is read with the next. A text delta older than an event already relayed is
//! dropped. Once the listener connects again, every followed run is read
//! again: notices sent meanwhile were lost. Only runs this process's
//! subscribers follow are read.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mimorya_db::Database;
use mimorya_db::notifications::listen_run_events;
use mimorya_db::repositories::run_events::list_run_events;
use tokio::sync::watch;
use uuid::Uuid;

use super::notice::{NoticeKind, delta_event};
use super::{BusEvent, Notice, RunEventBus};

/// The listener's connection, as an operator finds it.
pub const LISTENER_NAME: &str = "mimorya-run-events";
/// How long before connecting again after a failure.
const RETRY: Duration = Duration::from_secs(1);

pub struct Relay {
    bus: Arc<RunEventBus>,
    db: Database,
    /// The last event relayed, by run followed here.
    relayed: HashMap<Uuid, i32>,
}

impl Relay {
    pub fn new(bus: Arc<RunEventBus>, db: Database) -> Self {
        Self {
            bus,
            db,
            relayed: HashMap::new(),
        }
    }

    /// Listens to the other processes on `url` until `stopping`, connecting
    /// again whenever the connection is lost.
    pub async fn listen(mut self, url: String, mut stopping: watch::Receiver<bool>) {
        loop {
            let connected = tokio::select! {
                connected = listen_run_events(&url, LISTENER_NAME) => connected,
                _ = stopping.wait_for(|stop| *stop) => return,
            };
            match connected {
                Ok(mut listener) => {
                    // Connected: what was sent before is read from the journals.
                    self.resync().await;
                    loop {
                        let next = tokio::select! {
                            next = listener.try_recv() => next,
                            _ = stopping.wait_for(|stop| *stop) => return,
                        };
                        match next {
                            Ok(Some(notification)) => self.handle(notification.payload()).await,
                            // Connected again: what was sent meanwhile is lost.
                            Ok(None) => self.resync().await,
                            Err(error) => {
                                tracing::warn!(
                                    error = format!("{error:#}"),
                                    "Run events listener lost"
                                );
                                break;
                            }
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "Run events listener not connected"
                    );
                }
            }
            tokio::select! {
                () = tokio::time::sleep(RETRY) => {}
                _ = stopping.wait_for(|stop| *stop) => return,
            }
        }
    }

    /// Relays the notice `payload` carries, if another process sent it.
    pub async fn handle(&mut self, payload: &str) {
        let Some(notice) = Notice::decode(payload) else {
            return;
        };
        if notice.origin == self.bus.origin() {
            return;
        }
        let run_id = notice.run_id;
        let Some(tenant_id) = self.bus.follower_company(run_id) else {
            self.relayed.remove(&run_id);
            return;
        };
        match notice.kind {
            NoticeKind::Event { seq } => {
                let Some(after) = reads_after(&self.relayed, run_id, seq) else {
                    return;
                };
                self.relay_events(tenant_id, run_id, after).await;
            }
            NoticeKind::Delta { after, text } => {
                if admits_delta(&self.relayed, run_id, after) {
                    self.bus.publish(run_id, delta_event(&text));
                }
            }
        }
    }

    /// Reads again every run followed here, from its last event relayed.
    pub async fn resync(&mut self) {
        let followed = self.bus.followed();
        self.relayed
            .retain(|run, _| followed.iter().any(|(_, followed)| followed == run));
        for (tenant_id, run_id) in followed {
            let after = self.relayed.get(&run_id).copied().unwrap_or(0);
            self.relay_events(tenant_id, run_id, after).await;
        }
    }

    async fn relay_events(&mut self, tenant_id: Uuid, run_id: Uuid, after: i32) {
        let rows = match self.read(tenant_id, run_id, after).await {
            Ok(rows) => rows,
            Err(error) => {
                // Read again with the run's next notice.
                tracing::warn!(run_id = %run_id, error = format!("{error:#}"), "Run events not relayed");
                return;
            }
        };
        for (seq, event_type, data) in rows {
            self.relayed.insert(run_id, seq);
            let event = BusEvent {
                seq: Some(seq),
                event_type,
                data,
            };
            self.bus.publish(run_id, event);
        }
    }

    async fn read(
        &self,
        tenant_id: Uuid,
        run_id: Uuid,
        after: i32,
    ) -> anyhow::Result<Vec<(i32, String, serde_json::Value)>> {
        let mut tx = self.db.begin(tenant_id).await?;
        let rows = list_run_events(&mut tx, run_id, after).await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.seq, row.event_type, row.data))
            .collect())
    }
}

/// From which event to read the run's journal on a notice of its event
/// `seq`: the last relayed, or just before `seq` for a run newly followed
/// (its subscriber read the journal up to then); `None` once relayed.
fn reads_after(relayed: &HashMap<Uuid, i32>, run_id: Uuid, seq: i32) -> Option<i32> {
    match relayed.get(&run_id) {
        Some(&last) if seq <= last => None,
        Some(&last) => Some(last),
        None => Some(seq - 1),
    }
}

/// Whether a delta written after the run's event `after` is still to show:
/// not once a later event was relayed (it replaced the draft).
fn admits_delta(relayed: &HashMap<Uuid, i32>, run_id: Uuid, after: Option<i32>) -> bool {
    match (after, relayed.get(&run_id)) {
        (Some(after), Some(&last)) => after >= last,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_s_journal_is_read_from_its_last_event_relayed() {
        let run = Uuid::new_v4();
        let mut relayed = HashMap::new();
        // Newly followed: from the event announced.
        assert_eq!(reads_after(&relayed, run, 5), Some(4));
        relayed.insert(run, 5);
        assert_eq!(reads_after(&relayed, run, 5), None);
        // One lost in between is read with the next.
        assert_eq!(reads_after(&relayed, run, 8), Some(5));
    }

    #[test]
    fn a_delta_older_than_an_event_relayed_is_dropped() {
        let run = Uuid::new_v4();
        let mut relayed = HashMap::new();
        assert!(admits_delta(&relayed, run, Some(2)));
        relayed.insert(run, 3);
        assert!(admits_delta(&relayed, run, Some(3)));
        assert!(!admits_delta(&relayed, run, Some(2)));
        assert!(admits_delta(&relayed, run, None));
    }
}
