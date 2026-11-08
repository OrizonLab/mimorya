//! The file of a person's export (ADR-187): the JSON written once, then
//! sent a block at a time by a body that holds the export's place. The
//! place is given back when the last block is taken, when the body is
//! dropped (the person gone before the end), or when no block is taken
//! for `STALL` (a download paused, a client that no longer reads, ADR-188):
//! the file is then let go of and its sending ends in error. Never before:
//! axum writes a response after its handler returns, so a place given back
//! with the handler would let a process hold more files at once than it
//! may.

use std::io;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::body::{Body, Bytes};
use futures::stream;
use tokio::time::Instant;

use crate::contract::PersonalExport;
use crate::services::places::Place;

/// The bytes sent at a time.
const BLOCK: usize = 64 * 1024;

/// How long a file waits for its next block to be taken.
pub const STALL: Duration = Duration::from_secs(60);

/// An export, written, and its place.
pub struct ExportFile {
    /// The day it was made, `YYYY-MM-DD`, for the file's name.
    pub day: String,
    bytes: Bytes,
    place: Place,
}

impl ExportFile {
    pub(super) fn new(export: &PersonalExport, place: Place) -> anyhow::Result<Self> {
        let day = export.exported_at.get(..10).unwrap_or("export").to_owned();
        let bytes = Bytes::from(serde_json::to_vec(export)?);
        Ok(Self { day, bytes, place })
    }

    /// The file's size, in bytes.
    pub fn size(&self) -> usize {
        self.bytes.len()
    }

    /// The body that sends the file, holding its place until the end.
    pub fn into_body(self) -> Body {
        self.into_body_within(STALL)
    }

    fn into_body_within(self, stall: Duration) -> Body {
        let left = Left(Arc::new(Mutex::new(State::Sending {
            rest: self.bytes,
            _place: self.place,
            taken_at: Instant::now(),
        })));
        tokio::spawn(watch(left.clone(), stall));
        let blocks = stream::unfold(Sender(left), |sender| async move {
            let next = sender.0.next_block()?;
            Some((next, sender))
        });
        Body::from_stream(blocks)
    }
}

/// Where a file's sending is.
enum State {
    Sending {
        rest: Bytes,
        /// Held, never read: given back when the sending leaves this state.
        _place: Place,
        /// When its last block was taken.
        taken_at: Instant,
    },
    /// No block taken for too long: the file and its place let go of.
    Stalled,
    /// Sent, dropped, or its stall told.
    Over,
}

/// What is left to send, shared by the body and its watch.
#[derive(Clone)]
struct Left(Arc<Mutex<State>>);

impl Left {
    fn with<T>(&self, change: impl FnOnce(&mut State) -> T) -> T {
        change(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// The next block, the error that ends a stalled sending, or nothing
    /// once it is over (its place given back).
    fn next_block(&self) -> Option<io::Result<Bytes>> {
        self.with(|state| match state {
            State::Sending { rest, .. } if rest.is_empty() => {
                *state = State::Over;
                None
            }
            State::Sending { rest, taken_at, .. } => {
                *taken_at = Instant::now();
                Some(Ok(rest.split_to(rest.len().min(BLOCK))))
            }
            State::Stalled => {
                *state = State::Over;
                Some(Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "export file stalled",
                )))
            }
            State::Over => None,
        })
    }

    /// When the sending stalls if no block is taken before; `None` once it
    /// no longer sends.
    fn due(&self, stall: Duration) -> Option<Instant> {
        self.with(|state| match state {
            State::Sending { taken_at, .. } => Some(*taken_at + stall),
            _ => None,
        })
    }

    /// Lets the file and its place go if no block was taken for `stall`.
    fn stall_if_due(&self, stall: Duration) {
        self.with(|state| {
            if matches!(state, State::Sending { taken_at, .. } if taken_at.elapsed() >= stall) {
                *state = State::Stalled;
            }
        });
    }

    fn end(&self) {
        self.with(|state| *state = State::Over);
    }
}

/// The body's hold on what is left: dropped, it lets the file go.
struct Sender(Left);

impl Drop for Sender {
    fn drop(&mut self) {
        self.0.end();
    }
}

/// Watches a sending until it is over, ending it once stalled.
async fn watch(left: Left, stall: Duration) {
    while let Some(due) = left.due(stall) {
        tokio::time::sleep_until(due).await;
        left.stall_if_due(stall);
    }
}

#[cfg(test)]
mod tests;
