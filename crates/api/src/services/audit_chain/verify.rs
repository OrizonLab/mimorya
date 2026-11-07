//! The verification of a company's audit chain (ADR-178): its sealed
//! entries read in the chain's order, in one snapshot, each seal computed
//! again from the one recorded before it and compared. A place without an
//! entry (an entry removed) or an entry that is not as sealed (its content,
//! place or seal changed, or a place held twice) breaks the chain there;
//! the verification goes on from the seal recorded, so that every break is
//! counted, each missing place one, the first ones listed (missing places
//! that follow each other as one range). The entry that follows missing
//! places cannot be checked: its seal was computed from the last one's.
//!
//! What the database alone cannot show (seals taken away then set again by
//! the sealing on altered entries, entries removed from the chain's end),
//! an anchor does: a place and its seal logged by the sealing, outside the
//! database, which the chain must still hold. The entries still to seal
//! are counted, not verified.

use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{
    ChainHead, count_unsealed_audit_entries, sealed_audit_entries,
};
use uuid::Uuid;

use super::ChainKey;

/// Sealed entries read at a time.
const PAGE: usize = 1_000;
/// Breaks listed; the others are counted.
pub const LISTED_BREAKS: usize = 20;

/// Where a chain breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainBreak {
    /// No entry holds the places `from` to `to`: entries were removed.
    Missing { from: i64, to: i64 },
    /// The entry `entry_id`, at the place `seq`, is not as sealed.
    Altered { seq: i64, entry_id: i64 },
}

/// What the chain holds at an anchor's place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorFound {
    /// The anchor's seal, at its place.
    Held,
    /// Another seal: the chain was sealed again from before that place.
    Differs,
    /// Nothing: the chain ends before that place, or it is missing.
    Missing,
}

/// What a company's audit chain is found to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainReport {
    /// The entries whose seal holds.
    pub verified: i64,
    /// The entries not sealed yet.
    pub unsealed: i64,
    /// The last place read and its seal.
    pub head: Option<ChainHead>,
    /// The first breaks, at most `LISTED_BREAKS`.
    pub breaks: Vec<ChainBreak>,
    /// Every break: each altered entry, each missing place.
    pub broken: i64,
    /// What the chain holds at each anchor given, in their order.
    pub anchors: Vec<AnchorFound>,
}

impl ChainReport {
    /// Whether the chain breaks nowhere and holds every anchor.
    pub fn holds(&self) -> bool {
        self.broken == 0 && self.anchors.iter().all(|found| *found == AnchorFound::Held)
    }

    fn breaks_at(&mut self, at: ChainBreak) {
        self.broken += match at {
            ChainBreak::Missing { from, to } => to - from + 1,
            ChainBreak::Altered { .. } => 1,
        };
        if self.breaks.len() < LISTED_BREAKS {
            self.breaks.push(at);
        }
    }
}

/// Verifies the company's chain, and that it holds each of `anchors`.
pub async fn verify_company(
    db: &Database,
    key: &ChainKey,
    tenant_id: Uuid,
    anchors: &[ChainHead],
) -> anyhow::Result<ChainReport> {
    let mut tx = db.begin_snapshot(tenant_id).await?;
    let mut report = ChainReport {
        verified: 0,
        unsealed: count_unsealed_audit_entries(&mut tx).await?,
        head: None,
        breaks: Vec::new(),
        broken: 0,
        anchors: vec![AnchorFound::Missing; anchors.len()],
    };
    // The next place expected, and the seal recorded before it (unknown
    // after a missing place).
    let (mut expected, mut previous) = (1, Some(Vec::new()));
    let mut after = (0, 0);
    loop {
        let page = sealed_audit_entries(&mut tx, after, PAGE as i64).await?;
        for entry in &page {
            let (Some(seq), Some(hash)) = (entry.chain_seq, entry.chain_hash.as_deref()) else {
                // Read among the sealed entries: never without its seal.
                anyhow::bail!("audit entry {} read as sealed without its seal", entry.id);
            };
            after = (seq, entry.id);
            if seq < expected {
                report.breaks_at(ChainBreak::Altered {
                    seq,
                    entry_id: entry.id,
                });
                continue;
            }
            if seq > expected {
                report.breaks_at(ChainBreak::Missing {
                    from: expected,
                    to: seq - 1,
                });
                previous = None;
            }
            if let Some(previous) = &previous {
                let previous = (seq > 1).then_some(previous.as_slice());
                if key.seal(previous, seq, entry) == hash {
                    report.verified += 1;
                } else {
                    report.breaks_at(ChainBreak::Altered {
                        seq,
                        entry_id: entry.id,
                    });
                }
            }
            for (anchor, found) in anchors.iter().zip(report.anchors.iter_mut()) {
                if anchor.chain_seq == seq {
                    *found = if anchor.chain_hash == hash {
                        AnchorFound::Held
                    } else {
                        AnchorFound::Differs
                    };
                }
            }
            previous = Some(hash.to_vec());
            expected = seq + 1;
            report.head = Some(ChainHead {
                chain_seq: seq,
                chain_hash: hash.to_vec(),
            });
        }
        if page.len() < PAGE {
            return Ok(report);
        }
    }
}
