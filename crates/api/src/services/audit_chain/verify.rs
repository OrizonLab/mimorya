//! The verification of a company's audit chain (ADR-178): its sealed
//! entries read in the chain's order, in one snapshot, each seal computed
//! again from the previous one and compared. The first place without an
//! entry (an entry removed) or whose entry is not as sealed (its content,
//! place or seal changed, or a place held twice) breaks the chain, and the
//! verification stops there: what follows depends on it. The entries still
//! to seal are counted, not verified; nor is the chain's end, which only
//! the last place logged by the sealing shows.

use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{
    ChainHead, count_unsealed_audit_entries, sealed_audit_entries,
};
use uuid::Uuid;

use super::ChainKey;

/// Sealed entries read at a time.
const PAGE: usize = 1_000;

/// Where a chain breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainBreak {
    /// No entry holds the place `seq`: an entry was removed.
    Missing { seq: i64 },
    /// The entry `entry_id`, at the place `seq`, is not as sealed.
    Altered { seq: i64, entry_id: i64 },
}

/// What a company's audit chain is found to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainReport {
    /// The entries verified, from the first place.
    pub verified: i64,
    /// The entries not sealed yet.
    pub unsealed: i64,
    /// The last place verified and its seal.
    pub head: Option<ChainHead>,
    pub broken: Option<ChainBreak>,
}

pub async fn verify_company(
    db: &Database,
    key: &ChainKey,
    tenant_id: Uuid,
) -> anyhow::Result<ChainReport> {
    let mut tx = db.begin_snapshot(tenant_id).await?;
    let mut report = ChainReport {
        verified: 0,
        unsealed: count_unsealed_audit_entries(&mut tx).await?,
        head: None,
        broken: None,
    };
    let mut after = (0, 0);
    loop {
        let page = sealed_audit_entries(&mut tx, after, PAGE as i64).await?;
        for entry in &page {
            let expected = report.verified + 1;
            let (Some(seq), Some(hash)) = (entry.chain_seq, entry.chain_hash.as_deref()) else {
                // Read among the sealed entries: never without its seal.
                anyhow::bail!("audit entry {} read as sealed without its seal", entry.id);
            };
            if seq > expected {
                report.broken = Some(ChainBreak::Missing { seq: expected });
                return Ok(report);
            }
            let previous = report.head.as_ref().map(|head| head.chain_hash.as_slice());
            if seq < expected || key.seal(previous, seq, entry) != hash {
                report.broken = Some(ChainBreak::Altered {
                    seq,
                    entry_id: entry.id,
                });
                return Ok(report);
            }
            report.verified = seq;
            report.head = Some(ChainHead {
                chain_seq: seq,
                chain_hash: hash.to_vec(),
            });
            after = (seq, entry.id);
        }
        if page.len() < PAGE {
            return Ok(report);
        }
    }
}
