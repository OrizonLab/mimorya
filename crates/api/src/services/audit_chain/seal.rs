//! The sealing of the audit log (ADR-178): every minute, in a process that
//! executes runs and one process at a time (ADR-086), each company's
//! entries still to seal are sealed in the order they were written, after
//! its chain's last place, by batches each in its own transaction, at most
//! `MAX_BATCHES` a pass so that a long backlog leaves the other sweeps
//! their turn. The chain's last place and seal is then logged: kept apart
//! from the database, it shows entries removed from the chain's end, which
//! nothing after them seals. A company failing leaves the others sealed.

use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{
    ChainEntry, Seal, audit_chain_head, audit_log_tenants, lock_audit_chain, seal_audit_entries,
    unsealed_audit_entries,
};
use uuid::Uuid;

use super::{ChainKey, hex};

/// The sweep's lock (ADR-086).
pub const AUDIT_CHAIN_SWEEP: &str = "audit-chain";
/// Entries sealed in one transaction.
const BATCH: usize = 500;
/// Transactions of a company in one pass.
const MAX_BATCHES: usize = 20;

/// Seals the entries still to seal of every company; how many. None while
/// another process seals.
pub async fn seal_audit_log(db: &Database, key: &ChainKey) -> anyhow::Result<usize> {
    let Some(sweep) = db.try_sweep_lock(AUDIT_CHAIN_SWEEP).await? else {
        return Ok(0);
    };
    let sealed = seal_all(db, key).await;
    sweep.release().await;
    sealed
}

async fn seal_all(db: &Database, key: &ChainKey) -> anyhow::Result<usize> {
    let mut sealed = 0;
    for tenant_id in audit_log_tenants(db.pool(), true).await? {
        match seal_company(db, key, tenant_id).await {
            Ok(count) => sealed += count,
            Err(error) => tracing::warn!(
                tenant_id = %tenant_id,
                error = format!("{error:#}"),
                "Audit log entries not sealed"
            ),
        }
    }
    Ok(sealed)
}

/// Seals the company's entries still to seal, at most `MAX_BATCHES`
/// batches; how many.
pub async fn seal_company(db: &Database, key: &ChainKey, tenant_id: Uuid) -> anyhow::Result<usize> {
    let mut sealed = 0;
    let mut head = None;
    for _ in 0..MAX_BATCHES {
        let mut tx = db.begin(tenant_id).await?;
        lock_audit_chain(&mut tx, tenant_id).await?;
        let last = audit_chain_head(&mut tx).await?;
        let entries = unsealed_audit_entries(&mut tx, BATCH as i64).await?;
        let Some(seals) = seals_after(key, last.map(|h| (h.chain_seq, h.chain_hash)), &entries)
        else {
            break;
        };
        seal_audit_entries(&mut tx, tenant_id, &seals).await?;
        tx.commit().await?;
        sealed += seals.len();
        head = seals.last().map(|seal| (seal.seq, hex(&seal.hash)));
        if entries.len() < BATCH {
            break;
        }
    }
    if let Some((seq, hash)) = head {
        tracing::info!(tenant_id = %tenant_id, seq, head = %hash, "Audit log sealed");
    }
    Ok(sealed)
}

/// The seals of `entries`, in order after the place and seal `last`; none
/// when there is nothing to seal.
fn seals_after(
    key: &ChainKey,
    last: Option<(i64, Vec<u8>)>,
    entries: &[ChainEntry],
) -> Option<Vec<Seal>> {
    if entries.is_empty() {
        return None;
    }
    let (mut seq, mut previous) = match last {
        Some((seq, hash)) => (seq, Some(hash)),
        None => (0, None),
    };
    let seals = entries
        .iter()
        .map(|entry| {
            seq += 1;
            let hash = key.seal(previous.as_deref(), seq, entry).to_vec();
            previous = Some(hash.clone());
            Seal {
                id: entry.id,
                seq,
                hash,
            }
        })
        .collect();
    Some(seals)
}
