//! What a live instance whose session closed for good becomes (ADR-163):
//! a local server that exited, or a server that no longer answers a ping
//! (ADR-164). Released and started again by the run that finds it closed,
//! unless it lived less than `QUICK_EXIT`. It is then a failed start,
//! remembered as one: a server that exits, or stops answering, as soon as
//! started is not started again by every run. Its life lasts until its
//! session ended, when the session dates its end (ADR-165), or else until
//! a run finds it closed. A newer instance about to serve a run that read
//! an older version is judged so too (ADR-177).

use std::time::Duration;
use tokio::time::Instant;

use super::{RETRY_AFTER, SessionKey, Slot, State, Unavailable, Version, release};

/// A session that lived less than this was a failed start.
pub const QUICK_EXIT: Duration = RETRY_AFTER;

/// Why an instance whose session lived less than `QUICK_EXIT` is not started
/// again at once.
pub const EXITED_AT_ONCE: &str = "Le serveur a cessé de répondre moins d’une minute après son \
     démarrage : il n’est pas redémarré avant une minute";

/// Releases the live instance that would serve a run that read `version`
/// if its session closed for good: of that version, or, for a reading that
/// is not stale, a newer one; the run then starts the version it read.
/// Remembered as a failed start of its own version, and told so, if it
/// lived less than `QUICK_EXIT`, the failure then logged by the caller that
/// found it, a run or a start. A stale reading leaves a newer instance to
/// the runs of its own version, which record its failure on the connector.
pub(super) fn closed(
    state: &mut State,
    id: SessionKey,
    version: Version,
    stale: bool,
) -> Option<Unavailable> {
    let (lived, version) = match state.slots.get(&id) {
        Some(Slot::Live {
            version: v,
            runtime,
            since,
            ..
        }) if (*v == version || (*v > version && !stale)) && runtime.is_closed() => {
            let lived = match runtime.ended_at() {
                Some(ended) => ended.saturating_duration_since(*since),
                None => since.elapsed(),
            };
            (lived, *v)
        }
        _ => return None,
    };
    release(state.slots.remove(&id));
    if lived >= QUICK_EXIT {
        let connector = id.instance;
        tracing::warn!(%connector, "A kept connector's session closed: released");
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
