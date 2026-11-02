//! What a live instance whose session closed for good becomes (ADR-163):
//! a local server that exited, or a server that no longer answers a ping
//! (ADR-164). Released and started again by the run that finds it closed,
//! unless that run finds it closed within `QUICK_EXIT` of its start. It is
//! then a failed start, remembered as one: a server that exits, or stops
//! answering, as soon as started is not started again by every run that
//! comes within the minute. The time is the one the session is found closed
//! at, not the one it closed at: runs that come more than a minute apart
//! each start it again.

use chrono::{DateTime, Utc};
use std::time::Duration;
use tokio::time::Instant;
use uuid::Uuid;

use super::{RETRY_AFTER, Slot, State, Unavailable, release};

/// A session found closed within this time of its start was a failed start.
pub const QUICK_EXIT: Duration = RETRY_AFTER;

/// Why an instance whose session was found closed within `QUICK_EXIT` of its
/// start is not started again at once.
pub const EXITED_AT_ONCE: &str = "Le serveur a cessé de répondre moins d’une minute après son \
     démarrage : il n’est pas redémarré avant une minute";

/// Releases the live instance of this version if its session closed for
/// good, to be started again; remembered as a failed start, and told so,
/// if found closed within `QUICK_EXIT` of its start.
pub(super) fn closed(state: &mut State, id: Uuid, version: DateTime<Utc>) -> Option<Unavailable> {
    let since = match state.slots.get(&id) {
        Some(Slot::Live {
            version: v,
            runtime,
            since,
        }) if *v == version && runtime.is_closed() => *since,
        _ => return None,
    };
    release(state.slots.remove(&id));
    if since.elapsed() >= QUICK_EXIT {
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
