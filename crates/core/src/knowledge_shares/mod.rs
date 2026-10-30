//! Knowledge shared between units (ADR-143): a company's administrators of
//! its access policies share a unit's knowledge (the memory filed in that
//! unit's space, the documents deposited there, either or both) with
//! another unit, and if they say so the units under it, up to a
//! sensitivity, until a date if they set one. Its members then read that
//! knowledge as they read their own unit's, never above the share's
//! sensitivity nor their own clearance. Nothing else crosses: neither
//! writing, nor the units under the unit shared, nor anyone's personal
//! memory; and only for whom reads their own unit's knowledge
//! (`memory:read`, `document:read`) at all. What a share may say is in
//! `rules`.

mod rules;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::authz::{OrgTree, Sensitivity, Subject};

pub use rules::{ShareProblem, share_problem};

/// The knowledge a share opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedKnowledge {
    /// The memory filed in the unit's space.
    Memory,
    /// The documents deposited in the unit's space.
    Documents,
}

/// A unit's knowledge shared with another unit: what the share says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeShare {
    /// The unit whose space is shared.
    pub source_unit_id: Uuid,
    /// The unit whose members read it.
    pub target_unit_id: Uuid,
    /// Whether the members of the units under the target read it too.
    pub include_descendants: bool,
    pub memory: bool,
    pub documents: bool,
    /// The highest sensitivity read through it.
    pub ceiling: Sensitivity,
    pub expires_at: Option<DateTime<Utc>>,
}

impl KnowledgeShare {
    fn opens(&self, kind: SharedKnowledge) -> bool {
        match kind {
            SharedKnowledge::Memory => self.memory,
            SharedKnowledge::Documents => self.documents,
        }
    }

    fn in_force(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_none_or(|at| at > now)
    }

    /// Whether a member of `units` is one of those it is shared with.
    fn reaches(&self, tree: &OrgTree, units: &[Uuid]) -> bool {
        units.iter().any(|unit| {
            *unit == self.target_unit_id
                || (self.include_descendants
                    && (tree.ancestors(*unit).iter()).any(|a| a.id == self.target_unit_id))
        })
    }
}

/// The units whose `kind` knowledge `subject` reads through `shares` in
/// force at `now`, each with the highest sensitivity read there: the
/// share's, never above their clearance, the highest of several shares of
/// a unit. In the organisation's order of the units shared.
pub fn shared_units(
    shares: &[KnowledgeShare],
    tree: &OrgTree,
    subject: &Subject,
    (kind, now): (SharedKnowledge, DateTime<Utc>),
) -> Vec<(Uuid, Sensitivity)> {
    let clearance = subject.clearance();
    let mut reached: HashMap<Uuid, Sensitivity> = HashMap::new();
    let open = shares.iter().filter(|share| {
        share.opens(kind) && share.in_force(now) && share.reaches(tree, &subject.org_unit_ids)
    });
    for share in open {
        let read = share.ceiling.min(clearance);
        let highest = reached.entry(share.source_unit_id).or_insert(read);
        *highest = (*highest).max(read);
    }
    let mut units: Vec<(Uuid, Sensitivity)> = reached.into_iter().collect();
    units.sort_by_key(|(unit, _)| tree.all().iter().position(|u| u.id == *unit));
    units
}

#[cfg(test)]
mod tests;
