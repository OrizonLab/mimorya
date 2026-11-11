//! The verification of its company's audit chain an administrator asks for
//! (`audit:read`, company-wide, ADR-179), against the anchors they kept
//! outside the platform, and what it found.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use validator::Validate;

use crate::http::iso;
use crate::services::audit_chain::{
    AnchorFound, ChainBreak, MAX_ANCHORS, OwnChainCheck, anchor_text, hex,
};

/// Anchors a verification takes, as the validation counts them.
const ANCHORS_TAKEN: u64 = MAX_ANCHORS as u64;

/// Body of `POST /admin/audit/verifications`.
#[derive(Debug, Deserialize, Validate, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditChainCheck {
    /// Anchors kept outside the platform, as the verification shows them
    /// or the sealing logs them: `<company>:<place>:<seal>`, the company by
    /// its identifier or its slug.
    #[validate(length(max = ANCHORS_TAKEN))]
    pub anchors: Vec<String>,
}

/// A place of the chain and its seal, with the anchor that keeps it.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditChainPlace {
    pub place: i64,
    /// In hexadecimal.
    pub seal: String,
    /// `<company>:<place>:<seal>`, the company by its identifier.
    pub anchor: String,
}

/// Where the chain breaks.
#[derive(Debug, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditChainBreak {
    /// No entry holds the places `from` to `to`: entries were removed.
    Missing { from: i64, to: i64 },
    /// The entry at the place is not as it was sealed.
    Altered {
        place: i64,
        #[serde(rename = "entryId")]
        entry_id: i64,
    },
}

/// What the chain holds at an anchor's place.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AuditAnchorFound {
    /// The anchor's seal.
    Held,
    /// Another seal: the chain was sealed again from before that place.
    Differs,
    /// Nothing: the chain ends before that place, or it is missing.
    Missing,
    /// Before the chain's start: the place went with the entries five
    /// years old (ADR-198).
    Purged,
}

/// An anchor given, and what the chain holds at its place.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditAnchorCheck {
    pub place: i64,
    pub seal: String,
    pub found: AuditAnchorFound,
}

/// `POST /admin/audit/verifications`.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AuditChainVerification {
    /// Whether the chain breaks nowhere and holds every anchor given.
    pub holds: bool,
    /// Entries whose seal holds.
    pub verified: i64,
    /// Entries not sealed yet: sealed within a minute or so.
    pub unsealed: i64,
    /// The last sealed place: the anchor to keep.
    pub last: Option<AuditChainPlace>,
    /// The chain's first place: after 1 once the entries five years old
    /// went (ADR-198).
    pub first_place: i64,
    /// The places deleted with them, in all.
    pub purged: i64,
    /// Every break: each altered entry, each missing place.
    pub broken: i64,
    /// The first breaks.
    pub breaks: Vec<AuditChainBreak>,
    /// In the order they were given.
    pub anchors: Vec<AuditAnchorCheck>,
    pub verified_at: String,
}

impl AuditChainVerification {
    pub fn of(check: &OwnChainCheck, now: DateTime<Utc>) -> Self {
        let report = &check.report;
        Self {
            holds: report.holds(),
            verified: report.verified,
            unsealed: report.unsealed,
            last: report.head.as_ref().map(|head| AuditChainPlace {
                place: head.chain_seq,
                seal: hex(&head.chain_hash),
                anchor: anchor_text(check.tenant_id, head),
            }),
            first_place: report.start.as_ref().map_or(1, |start| start.first_seq),
            purged: report
                .start
                .as_ref()
                .map_or(0, |start| start.purged_entries),
            broken: report.broken,
            breaks: report
                .breaks
                .iter()
                .map(|at| match *at {
                    ChainBreak::Missing { from, to } => AuditChainBreak::Missing { from, to },
                    ChainBreak::Altered { seq, entry_id } => AuditChainBreak::Altered {
                        place: seq,
                        entry_id,
                    },
                })
                .collect(),
            anchors: check
                .anchors
                .iter()
                .zip(&report.anchors)
                .map(|(anchor, found)| AuditAnchorCheck {
                    place: anchor.seq,
                    seal: hex(&anchor.seal),
                    found: match found {
                        AnchorFound::Held => AuditAnchorFound::Held,
                        AnchorFound::Differs => AuditAnchorFound::Differs,
                        AnchorFound::Missing => AuditAnchorFound::Missing,
                        AnchorFound::Purged => AuditAnchorFound::Purged,
                    },
                })
                .collect(),
            verified_at: iso(&now),
        }
    }
}
