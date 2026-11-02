//! What a live instance whose session closed for good becomes (ADR-163):
//! a local server that exited, or a server that no longer answers a ping
//! (ADR-164). Released and started again by the run that finds it closed,
//! unless it lived less than `QUICK_EXIT`. It is then a failed start,
//! remembered as one: a server that exits, or stops answering, as soon as
//! started is not started again by every run. Its life lasts until its
//! session ended, when the session dates its end (ADR-165), or else until
//! a run finds it closed.

use chrono::{DateTime, Utc};
use std::time::Duration;
use tokio::time::Instant;
use uuid::Uuid;

use super::{RETRY_AFTER, Slot, State, Unavailable, release};

/// A session that lived less than this was a failed start.
pub const QUICK_EXIT: Duration = RETRY_AFTER;

/// Why an instance whose session lived less than `QUICK_EXIT` is not started
/// again at once.
pub const EXITED_AT_ONCE: &str = "Le serveur a cessé de répondre moins d’une minute après son \
     démarrage : il n’est pas redémarré avant une minute";

/// Releases the live instance of this version if its session closed for
/// good, to be started again; remembered as a failed start, and told so,
/// if it lived less than `QUICK_EXIT`.
pub(super) fn closed(state: &mut State, id: Uuid, version: DateTime<Utc>) -> Option<Unavailable> {
    let lived = match state.slots.get(&id) {
        Some(Slot::Live {
            version: v,
            runtime,
            since,
        }) if *v == version && runtime.is_closed() => match runtime.ended_at() {
            Some(ended) => ended.saturating_duration_since(*since),
            None => since.elapsed(),
        },
        _ => return None,
    };
    release(state.slots.remove(&id));
    if lived >= QUICK_EXIT {
        tracing::warn!(connector = %id, "A kept connector's session closed: starting it again");
        return None;
    }
    let failed = Slot::Failed {
        version,
        until: Instant::now() + RETRY_AFTER,
        message: EXITED_AT_ONCE.into(),
    };
    state.slots.insert(id, failed);
    Some(Unavailable::new(EXITED_AT_ONCE))
}
