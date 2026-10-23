//! Text deltas on their way to the other processes (ADR-084), sent in the
//! order the model wrote them, outside any transaction: they are not
//! recorded. Consecutive deltas of a run are sent together; none while no
//! other process listens (a single instance), which is checked again every
//! few seconds.

use std::time::{Duration, Instant};

use mimorya_db::Database;
use mimorya_db::notifications::{notify_run_events, run_event_listeners};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::Notice;

/// Deltas waiting to be sent, beyond which new ones are dropped.
pub(super) const CAPACITY: usize = 1024;
/// Bytes of text sent together at most.
const BATCH_TEXT: usize = 4000;
/// How long whether another process listens holds.
const LISTENERS_HOLD: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq)]
pub struct Delta {
    pub run_id: Uuid,
    /// The last event the run recorded before it, if known.
    pub after: Option<i32>,
    pub text: String,
}

/// The deltas a process hands over to the others.
pub struct Outbox {
    deltas: mpsc::Receiver<Delta>,
    /// Taken out while gathering a batch, for the next one.
    next: Option<Delta>,
    /// Whether another process listened, and when that was checked.
    others: Option<(Instant, bool)>,
}

impl Outbox {
    pub(super) fn new(deltas: mpsc::Receiver<Delta>) -> Self {
        Self {
            deltas,
            next: None,
            others: None,
        }
    }

    /// Sends the deltas, as `origin`'s, until the bus is gone; `listens`
    /// says whether this process listens too (a worker does not, ADR-085).
    pub async fn send_all(mut self, db: Database, origin: Uuid, listens: bool) {
        while let Some(delta) = self.next_batch().await {
            if !self.others_listen(&db, listens).await {
                continue;
            }
            let payloads = Notice::deltas(origin, delta.run_id, delta.after, &delta.text);
            for payload in payloads {
                if let Err(error) = notify_run_events(db.pool(), &payload).await {
                    // Lost: the run's next message replaces the draft.
                    tracing::debug!(run_id = %delta.run_id, error = format!("{error:#}"), "Run text delta not sent");
                }
            }
        }
    }

    /// Whether another process than this one listens to the run events.
    async fn others_listen(&mut self, db: &Database, listens: bool) -> bool {
        if let Some((checked, others)) = self.others
            && checked.elapsed() < LISTENERS_HOLD
        {
            return others;
        }
        // This one's listener among them, when it listens and is connected.
        let others = match run_event_listeners(db.pool()).await {
            Ok(listeners) => listeners > i64::from(listens),
            Err(error) => {
                tracing::debug!(
                    error = format!("{error:#}"),
                    "Run event listeners not counted"
                );
                false
            }
        };
        self.others = Some((Instant::now(), others));
        others
    }

    /// The next delta, with those of the same run that follow it.
    pub(super) async fn next_batch(&mut self) -> Option<Delta> {
        let mut batch = match self.next.take() {
            Some(delta) => delta,
            None => self.deltas.recv().await?,
        };
        while let Ok(delta) = self.deltas.try_recv() {
            let joins = delta.run_id == batch.run_id
                && delta.after == batch.after
                && batch.text.len() + delta.text.len() <= BATCH_TEXT;
            if !joins {
                self.next = Some(delta);
                break;
            }
            batch.text.push_str(&delta.text);
        }
        Some(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(run_id: Uuid, after: Option<i32>, text: &str) -> Delta {
        Delta {
            run_id,
            after,
            text: text.into(),
        }
    }

    #[tokio::test]
    async fn consecutive_deltas_of_a_run_go_together_in_order() {
        let (sender, receiver) = mpsc::channel(CAPACITY);
        let mut outbox = Outbox::new(receiver);
        let (one, other) = (Uuid::new_v4(), Uuid::new_v4());
        for queued in [
            delta(one, Some(1), "Bon"),
            delta(one, Some(1), "jour"),
            delta(one, Some(2), " !"),
            delta(other, Some(2), "Salut"),
        ] {
            sender.send(queued).await.unwrap();
        }
        drop(sender);
        let mut sent = Vec::new();
        while let Some(batch) = outbox.next_batch().await {
            sent.push((batch.run_id, batch.after, batch.text));
        }
        assert_eq!(
            sent,
            [
                (one, Some(1), "Bonjour".to_string()),
                (one, Some(2), " !".to_string()),
                (other, Some(2), "Salut".to_string()),
            ]
        );
    }
}
