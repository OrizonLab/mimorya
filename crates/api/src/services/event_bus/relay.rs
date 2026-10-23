//! The other processes' run events, relayed to this process's subscribers
//! (ADR-084). A notice of a recorded event is read back from the run's
//! journal, in the company of whom follows the run here (row-level security
//! applies: the notice names no company), from the last event they all
//! have: one lost in between is read with the next. A text delta older than
//! an event they have is dropped. Every followed run is read again once the
//! listener connects again (`listener.rs`), and after a read failed: the
//! run's next notice may never come (its end). Only runs this process's
//! subscribers follow are read.

use std::sync::Arc;

use mimorya_db::Database;
use mimorya_db::repositories::run_events::list_run_events;
use uuid::Uuid;

use super::notice::{NoticeKind, delta_event};
use super::{BusEvent, Followed, Notice, RunEventBus};

pub struct Relay {
    pub(super) bus: Arc<RunEventBus>,
    db: Database,
}

impl Relay {
    pub fn new(bus: Arc<RunEventBus>, db: Database) -> Self {
        Self { bus, db }
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
        self.bus.set_relay_behind(false);
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
                self.bus.set_relay_behind(true);
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
