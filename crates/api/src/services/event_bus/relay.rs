//! The other processes' run events, relayed to this process's subscribers
//! (ADR-084). A notice of a recorded event is read back from the run's
//! journal, in the company of whom follows the run here (row-level security
//! applies: the notice names no company), from the last event they all
//! have: one lost in between is read with the next. A text delta older than
//! an event they have is dropped. Once the listener connects again, every
//! followed run is read again: notices sent meanwhile were lost; so is it
//! a moment after a read failed, the run's next notice perhaps never coming
//! (its end). Only runs this process's subscribers follow are read.

use std::sync::Arc;
use std::time::Duration;

use mimorya_db::Database;
use mimorya_db::notifications::{RUN_EVENTS_LISTENER, listen_run_events};
use mimorya_db::repositories::run_events::list_run_events;
use tokio::sync::watch;
use uuid::Uuid;

use super::notice::{NoticeKind, delta_event};
use super::{BusEvent, Followed, Notice, RunEventBus};

/// How long before connecting again after a failure, doubled after each
/// one up to `RETRY_AT_MOST`; how long before reading again after a read
/// failed.
const RETRY: Duration = Duration::from_secs(1);
const RETRY_AT_MOST: Duration = Duration::from_secs(30);

pub struct Relay {
    bus: Arc<RunEventBus>,
    db: Database,
    /// A read failed since the last time every followed run was read.
    behind: bool,
}

impl Relay {
    pub fn new(bus: Arc<RunEventBus>, db: Database) -> Self {
        Self {
            bus,
            db,
            behind: false,
        }
    }

    /// Listens to the other processes on `url` until `stopping`, connecting
    /// again whenever the connection is lost.
    pub async fn listen(mut self, url: String, mut stopping: watch::Receiver<bool>) {
        let mut retry = RETRY;
        loop {
            let connected = tokio::select! {
                connected = listen_run_events(&url, RUN_EVENTS_LISTENER) => connected,
                _ = stopping.wait_for(|stop| *stop) => return,
            };
            match connected {
                Ok(mut listener) => {
                    retry = RETRY;
                    // Connected: what was sent before is read from the journals.
                    self.resync().await;
                    self.bus.set_relaying(true);
                    loop {
                        let next = tokio::select! {
                            next = listener.try_recv() => Some(next),
                            () = tokio::time::sleep(RETRY), if self.behind => None,
                            _ = stopping.wait_for(|stop| *stop) => return,
                        };
                        match next {
                            Some(Ok(Some(notification))) => {
                                self.handle(notification.payload()).await;
                            }
                            // Connected again: what was sent meanwhile is
                            // lost. Or a read failed: read again.
                            Some(Ok(None)) | None => self.resync().await,
                            Some(Err(error)) => {
                                tracing::warn!(
                                    error = format!("{error:#}"),
                                    "Run events listener lost"
                                );
                                break;
                            }
                        }
                    }
                    self.bus.set_relaying(false);
                }
                Err(error) => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "Run events listener not connected"
                    );
                }
            }
            tokio::select! {
                () = tokio::time::sleep(retry) => {}
                _ = stopping.wait_for(|stop| *stop) => return,
            }
            retry = (retry * 2).min(RETRY_AT_MOST);
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
        let Some(followed) = self.bus.followed_run(notice.run_id) else {
            return;
        };
        match notice.kind {
            NoticeKind::Event { seq } => {
                if let Some(after) = reads_after(followed.handed, seq) {
                    self.relay_events(followed, after).await;
                }
            }
            NoticeKind::Delta { after, text } => {
                if admits_delta(followed.handed, after) {
                    self.bus.publish(followed.run_id, delta_event(&text));
                }
            }
        }
    }

    /// Reads again every run followed here, from the last event its
    /// subscribers all have.
    pub async fn resync(&mut self) {
        self.behind = false;
        for followed in self.bus.followed() {
            self.relay_events(followed, followed.handed.unwrap_or(0))
                .await;
        }
    }

    async fn relay_events(&mut self, followed: Followed, after: i32) {
        let Followed {
            tenant_id, run_id, ..
        } = followed;
        let rows = match self.read(tenant_id, run_id, after).await {
            Ok(rows) => rows,
            Err(error) => {
                self.behind = true;
                tracing::warn!(run_id = %run_id, error = format!("{error:#}"), "Run events not relayed");
                return;
            }
        };
        for (seq, event_type, data) in rows {
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
/// `seq`: the last its subscribers all have, or just before `seq` before
/// their stream said what it replayed; `None` once they have it, or for a
/// number no journal holds.
fn reads_after(handed: Option<i32>, seq: i32) -> Option<i32> {
    match handed {
        _ if seq < 1 => None,
        Some(last) if seq <= last => None,
        Some(last) => Some(last),
        None => Some(seq - 1),
    }
}

/// Whether a delta written after the run's event `after` is still to show:
/// not once its subscribers have a later event (it replaced the draft).
fn admits_delta(handed: Option<i32>, after: Option<i32>) -> bool {
    match (after, handed) {
        (Some(after), Some(last)) => after >= last,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_s_journal_is_read_from_the_last_event_its_subscribers_have() {
        // Before their stream said what it replayed: from the event announced.
        assert_eq!(reads_after(None, 5), Some(4));
        assert_eq!(reads_after(Some(5), 5), None);
        // One lost in between is read with the next.
        assert_eq!(reads_after(Some(5), 8), Some(5));
        // No journal holds it.
        assert_eq!(reads_after(None, i32::MIN), None);
        assert_eq!(reads_after(None, 0), None);
    }

    #[test]
    fn a_delta_older_than_an_event_they_have_is_dropped() {
        assert!(admits_delta(None, Some(2)));
        assert!(admits_delta(Some(3), Some(3)));
        assert!(!admits_delta(Some(3), Some(2)));
        assert!(admits_delta(Some(3), None));
    }
}
