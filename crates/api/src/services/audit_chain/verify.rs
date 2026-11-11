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
//!
//! A chain whose oldest places were deleted, five years old (ADR-198), is
//! verified from where it starts now: its first place, from the seal of
//! the one before it, which the purge kept. An anchor at that place is
//! checked against that seal; one before it, gone with what it sealed, is
//! said to be so.

use mimorya_db::Database;
use mimorya_db::repositories::audit_chain::{
    ChainHead, ChainStart, audit_chain_start, count_unsealed_audit_entries, sealed_audit_entries,
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
    /// Before the chain's start: the place went with the entries five
    /// years old (ADR-198).
    Purged,
}

impl AnchorFound {
    /// Whether the chain holds it, or let it go with its oldest places.
    pub fn holds(self) -> bool {
        matches!(self, Self::Held | Self::Purged)
    }
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
    /// Where the chain starts, if its oldest places were deleted.
    pub start: Option<ChainStart>,
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
        self.broken == 0 && self.anchors.iter().all(|found| found.holds())
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
    let start = audit_chain_start(&mut tx).await?;
    let mut report = ChainReport {
        verified: 0,
        unsealed: count_unsealed_audit_entries(&mut tx).await?,
        head: None,
        start: start.clone(),
        breaks: Vec::new(),
        broken: 0,
        anchors: anchors
            .iter()
            .map(|anchor| before_start(anchor, start.as_ref()))
            .collect(),
    };
    // The next place expected, and the seal recorded before it (unknown
    // after a missing place): the chain's first place, from the seal the
    // purge kept, if its oldest places went.
    let (mut expected, mut previous) = match &start {
        Some(start) => (start.first_seq, Some(start.previous_hash.clone())),
        None => (1, Some(Vec::new())),
    };
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

/// What the chain holds at an anchor's place before reading its entries:
/// at the place before its start, the seal the purge kept; before that,
/// nothing any more. Otherwise missing, until an entry is read there.
fn before_start(anchor: &ChainHead, start: Option<&ChainStart>) -> AnchorFound {
    let Some(start) = start else {
        return AnchorFound::Missing;
    };
    if anchor.chain_seq == start.first_seq - 1 {
        if anchor.chain_hash == start.previous_hash {
            AnchorFound::Held
        } else {
            AnchorFound::Differs
        }
    } else if anchor.chain_seq < start.first_seq {
        AnchorFound::Purged
    } else {
        AnchorFound::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(seq: i64, byte: u8) -> ChainHead {
        ChainHead {
            chain_seq: seq,
            chain_hash: vec![byte; 32],
        }
    }

    #[test]
    fn an_anchor_before_the_start_is_purged_at_its_eve_checked() {
        let start = ChainStart {
            first_seq: 10,
            previous_hash: vec![9; 32],
            purged_entries: 9,
        };
        assert_eq!(before_start(&head(3, 3), Some(&start)), AnchorFound::Purged);
        assert_eq!(before_start(&head(9, 9), Some(&start)), AnchorFound::Held);
        assert_eq!(
            before_start(&head(9, 1), Some(&start)),
            AnchorFound::Differs
        );
        assert_eq!(
            before_start(&head(10, 1), Some(&start)),
            AnchorFound::Missing
        );
        assert_eq!(before_start(&head(3, 3), None), AnchorFound::Missing);
        assert!(AnchorFound::Purged.holds() && AnchorFound::Held.holds());
        assert!(!AnchorFound::Differs.holds() && !AnchorFound::Missing.holds());
    }
}
