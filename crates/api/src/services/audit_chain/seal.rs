//! The sealing of the audit log (ADR-178): every minute, in a process that
//! executes runs and one process at a time (ADR-086), each company's
//! entries still to seal are sealed in the order they were written, after
//! its chain's last place, by batches each in its own transaction: at most
//! `MAX_BATCHES` a company and `PASS_BATCHES` a pass, so that a long
//! backlog leaves the other sweeps their turn. Before each batch, the seal
//! this process remembers for the company must still be at its place
//! (`heads.rs`): otherwise the chain moved behind the sealing, which
//! refuses to go on for that company and logs it as an error. The last
//! place and seal set are logged, even when a later batch fails: kept apart
//! from the database, they are the anchors that show seals taken away and
//! entries removed from the chain's end. A company failing leaves the
//! others sealed.

use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{
    ChainEntry, ChainHead, Seal, audit_chain_head, audit_log_tenants, lock_audit_chain,
    seal_audit_entries, unsealed_audit_entries,
};
use uuid::Uuid;

use super::heads::{ChainMoved, SealedHeads};
use super::{ChainKey, hex};

/// The sweep's lock (ADR-086).
pub const AUDIT_CHAIN_SWEEP: &str = "audit-chain";
/// Entries sealed in one transaction.
const BATCH: usize = 500;
/// Transactions of a company in one pass.
const MAX_BATCHES: usize = 20;
/// Transactions of all companies in one pass.
const PASS_BATCHES: usize = 100;

/// Seals the entries still to seal of every company; how many. None while
/// another process seals.
pub async fn seal_audit_log(
    db: &Database,
    key: &ChainKey,
    heads: &SealedHeads,
) -> anyhow::Result<usize> {
    let Some(sweep) = db.try_sweep_lock(AUDIT_CHAIN_SWEEP).await? else {
        return Ok(0);
    };
    let sealed = seal_all(db, key, heads).await;
    sweep.release().await;
    sealed
}

async fn seal_all(db: &Database, key: &ChainKey, heads: &SealedHeads) -> anyhow::Result<usize> {
    let (mut sealed, mut budget) = (0, PASS_BATCHES);
    for tenant_id in audit_log_tenants(db.pool(), true).await? {
        if budget == 0 {
            break;
        }
        let pass = seal_batches(db, key, heads, tenant_id, budget.min(MAX_BATCHES)).await;
        sealed += pass.entries;
        budget -= pass.batches;
        match pass.failure {
            None => {}
            Some(error) if error.is::<ChainMoved>() => tracing::error!(
                tenant_id = %tenant_id,
                error = %error,
                "Audit chain moved behind the sealing: its entries are no longer sealed"
            ),
            Some(error) => tracing::warn!(
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
pub async fn seal_company(
    db: &Database,
    key: &ChainKey,
    heads: &SealedHeads,
    tenant_id: Uuid,
) -> anyhow::Result<usize> {
    let pass = seal_batches(db, key, heads, tenant_id, MAX_BATCHES).await;
    match pass.failure {
        Some(error) => Err(error),
        None => Ok(pass.entries),
    }
}

/// What a company's pass did, and why it stopped early.
struct Pass {
    entries: usize,
    batches: usize,
    failure: Option<anyhow::Error>,
}

async fn seal_batches(
    db: &Database,
    key: &ChainKey,
    heads: &SealedHeads,
    tenant_id: Uuid,
    batches: usize,
) -> Pass {
    let mut pass = Pass {
        entries: 0,
        batches: 0,
        failure: None,
    };
    let mut head = None;
    while pass.batches < batches {
        pass.batches += 1;
        match seal_batch(db, key, heads, tenant_id).await {
            Ok(Some((last, count))) => {
                pass.entries += count;
                head = Some(last);
                if count < BATCH {
                    break;
                }
            }
            Ok(None) => break,
            Err(error) => {
                pass.failure = Some(error);
                break;
            }
        }
    }
    if let Some(head) = head {
        let (seq, seal) = (head.chain_seq, hex(&head.chain_hash));
        tracing::info!(tenant_id = %tenant_id, seq, head = %seal, "Audit log sealed");
    }
    pass
}

/// Seals one batch in its transaction: the last place set and how many,
/// or nothing when there is nothing to seal.
async fn seal_batch(
    db: &Database,
    key: &ChainKey,
    heads: &SealedHeads,
    tenant_id: Uuid,
) -> anyhow::Result<Option<(ChainHead, usize)>> {
    let mut tx = db.begin(tenant_id).await?;
    lock_audit_chain(&mut tx, tenant_id).await?;
    heads.check(&mut tx, tenant_id).await?;
    let last = audit_chain_head(&mut tx).await?;
    let entries = unsealed_audit_entries(&mut tx, BATCH as i64).await?;
    let Some(seals) = seals_after(key, last.map(|h| (h.chain_seq, h.chain_hash)), &entries) else {
        return Ok(None);
    };
    seal_audit_entries(&mut tx, tenant_id, &seals).await?;
    tx.commit().await?;
    let Some(sealed) = seals.last() else {
        return Ok(None);
    };
    let head = ChainHead {
        chain_seq: sealed.seq,
        chain_hash: sealed.hash.clone(),
    };
    heads.remember(tenant_id, head.clone());
    Ok(Some((head, seals.len())))
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
