//! The audit log kept five years (DATA_MODEL §5, ADR-198), in the retention
//! pass, by the process that holds the chain's key: each company whose
//! chain's first place is five years old loses its oldest places, a prefix
//! of it alone, `batch` at most in all. From where the chain starts (its
//! start holding its seal, `start.rs`), each place follows the one before,
//! holds its seal, was five years old when sealed, and is never its last
//! sealed place: a gap, a place held twice, an altered entry or a recent
//! one stops it there, so that the verification still shows what is wrong.
//! The places are deleted, and where the chain then starts kept with its
//! own seal, by a database function that checks again what it can, under
//! the chain's lock, in the company's transaction; the company finds the
//! purge in its audit log. A company failing leaves the others purged.

use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{
    ChainEntry, ChainStart, PlacesPurged, audit_chain_head, audit_purge_candidates,
    five_years_ago_us, lock_audit_chain, purge_audit_places, sealed_audit_entries,
};
use mimorya_db::repositories::retention::PurgedAudit;
use uuid::Uuid;

use super::ChainKey;
use super::start::{Start, chain_start};

/// The places a statement deletes at most, the database's bound.
const MAX_BATCH: i32 = 5000;

/// Deletes the oldest places of the companies' chains five years old,
/// `batch` at most in all (from 1 to 5000), the companies whose chain
/// starts with the oldest entry first; what each lost.
pub async fn purge_old_places(
    db: &Database,
    key: &ChainKey,
    batch: i32,
) -> anyhow::Result<Vec<PurgedAudit>> {
    let mut left = i64::from(batch.clamp(1, MAX_BATCH));
    let mut lost = Vec::new();
    for tenant_id in audit_purge_candidates(db.pool()).await? {
        if left <= 0 {
            break;
        }
        match purge_company(db, key, tenant_id, left).await {
            Ok(Some(purged)) => {
                left -= i64::from(purged.entries);
                lost.push(purged);
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(
                tenant_id = %tenant_id,
                error = format!("{error:#}"),
                "Old audit entries not purged"
            ),
        }
    }
    Ok(lost)
}

/// Deletes the oldest places of the company's chain, `left` at most, in
/// its transaction under its chain's lock; what it lost, if anything.
async fn purge_company(
    db: &Database,
    key: &ChainKey,
    tenant_id: Uuid,
    left: i64,
) -> anyhow::Result<Option<PurgedAudit>> {
    let mut tx = db.begin(tenant_id).await?;
    lock_audit_chain(&mut tx, tenant_id).await?;
    let start = chain_start(&mut tx, key, tenant_id).await?;
    let Some(head) = audit_chain_head(&mut tx).await? else {
        return Ok(None);
    };
    let horizon_us = five_years_ago_us(&mut tx).await?;
    let first = start.as_ref().map_or(1, |start| start.first_seq);
    // One more than may go: a place held twice at the end is seen.
    let entries = sealed_audit_entries(&mut tx, (first - 1, i64::MAX), left + 1).await?;
    let bounds = Bounds {
        head: head.chain_seq,
        horizon_us,
        most: left,
    };
    let Some(prefix) = old_prefix(key, start.as_ref(), &entries, bounds) else {
        return Ok(None);
    };
    let mut places = PlacesPurged {
        first,
        through: prefix.through,
        previous_hash: prefix.seal,
        purged_entries: start.as_ref().map_or(0, |start| start.purged_entries) + prefix.count,
        seal: Vec::new(),
    };
    let next = ChainStart {
        first_seq: places.through + 1,
        previous_hash: places.previous_hash.clone(),
        purged_entries: places.purged_entries,
        seal: Vec::new(),
    };
    places.seal = key.seal_start(tenant_id, &next).to_vec();
    let entries = purge_audit_places(&mut tx, tenant_id, &places).await?;
    tx.commit().await?;
    Ok(Some(PurgedAudit {
        tenant_id,
        entries,
        through_seq: places.through,
    }))
}

/// What bounds the places that go: the chain's last sealed place, which
/// stays, the time five years ago, and how many may go.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    head: i64,
    horizon_us: i64,
    most: i64,
}

/// The places that go: through which, its seal, how many.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Prefix {
    through: i64,
    seal: Vec<u8>,
    count: i64,
}

/// The oldest places of `entries` (the chain's sealed entries from where
/// it starts, in its order) that may go, from that start: each at the
/// place after the one before, held once, holding its seal, five years
/// old, before the last sealed place; none if the first one is not so.
fn old_prefix(
    key: &ChainKey,
    start: Option<&Start>,
    entries: &[ChainEntry],
    bounds: Bounds,
) -> Option<Prefix> {
    let (first, mut previous) = match start {
        Some(start) => (start.first_seq, Some(start.previous_hash.clone())),
        None => (1, None),
    };
    let mut prefix = None;
    for (index, entry) in entries.iter().enumerate() {
        let (Some(seq), Some(hash)) = (entry.chain_seq, entry.chain_hash.as_deref()) else {
            break;
        };
        let count = seq - first + 1;
        let twice = entries
            .get(index + 1)
            .is_some_and(|next| next.chain_seq == Some(seq));
        let old = entry.occurred_us < bounds.horizon_us;
        let follows = count
            == prefix
                .as_ref()
                .map_or(1, |prefix: &Prefix| prefix.count + 1);
        if !follows || twice || !old || seq >= bounds.head || count > bounds.most {
            break;
        }
        if key.seal(previous.as_deref(), seq, entry) != hash {
            break;
        }
        previous = Some(hash.to_vec());
        prefix = Some(Prefix {
            through: seq,
            seal: hash.to_vec(),
            count,
        });
    }
    prefix
}

#[cfg(test)]
mod tests;
