//! Where a document is deposited (ADR-095), and who reads or writes there:
//! the company's space, or a unit's, as for shared memory. `document:read`
//! reads the units of its scope, one's own units and those above them, and
//! the company's space; `document:write` deposits and deletes in the units
//! of its scope, in the company's only with a company-wide scope. Neither
//! reaches a document above one's clearance; `document:write` is never
//! the agent's (non-delegable).

use std::collections::HashSet;

use uuid::Uuid;

use crate::authz::{Actor, AuthorizationEngine, DataScope, Sensitivity, Subject};
use crate::memory::{UnitFilter, readable_units};

const READ: &str = "document:read";
const WRITE: &str = "document:write";

/// A document's space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSpace {
    Company,
    Unit(Uuid),
}

impl DocumentSpace {
    /// The stored scopes: the company's, a unit's.
    pub const SCOPES: [&'static str; 2] = ["tenant", "org_unit"];

    /// The stored scope (`tenant`, `org_unit`).
    pub fn scope(self) -> &'static str {
        match self {
            Self::Company => Self::SCOPES[0],
            Self::Unit(_) => Self::SCOPES[1],
        }
    }

    pub fn org_unit_id(self) -> Option<Uuid> {
        match self {
            Self::Company => None,
            Self::Unit(id) => Some(id),
        }
    }

    /// The space a stored scope and unit name, if consistent.
    pub fn stored(scope: &str, org_unit_id: Option<Uuid>) -> Option<Self> {
        let space = org_unit_id.map_or(Self::Company, Self::Unit);
        (space.scope() == scope).then_some(space)
    }
}

/// The documents a subject may read, as the store applies it
/// (`repositories::documents`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentReadFilter {
    pub units: UnitFilter,
    /// The company's space.
    pub company: bool,
    /// The highest sensitivity read.
    pub clearance: Sensitivity,
}

pub fn document_read_filter(
    engine: &AuthorizationEngine,
    subject: &Subject,
    actor: &Actor,
) -> DocumentReadFilter {
    let clearance = subject.clearance();
    match engine.data_scope(subject, READ, actor) {
        DataScope::None => DocumentReadFilter {
            units: UnitFilter::Only(HashSet::new()),
            company: false,
            clearance,
        },
        DataScope::All => DocumentReadFilter {
            units: UnitFilter::All,
            company: true,
            clearance,
        },
        DataScope::Restricted { org_unit_ids, .. } => DocumentReadFilter {
            units: UnitFilter::Only(readable_units(engine, subject, org_unit_ids)),
            company: true,
            clearance,
        },
    }
}

/// The spaces the subject may deposit into: the company's, then the
/// units', in the organisation's order.
pub fn writable_spaces(
    engine: &AuthorizationEngine,
    subject: &Subject,
    actor: &Actor,
) -> Vec<DocumentSpace> {
    let units = engine.org_tree().all().iter().map(|unit| unit.id);
    match engine.data_scope(subject, WRITE, actor) {
        DataScope::None => Vec::new(),
        DataScope::All => std::iter::once(DocumentSpace::Company)
            .chain(units.map(DocumentSpace::Unit))
            .collect(),
        DataScope::Restricted { org_unit_ids, .. } => units
            .filter(|id| org_unit_ids.contains(id))
            .map(DocumentSpace::Unit)
            .collect(),
    }
}

/// Whether the subject may deposit into `space`, or delete from it, a
/// document of `sensitivity`.
pub fn can_write_document(
    engine: &AuthorizationEngine,
    subject: &Subject,
    space: DocumentSpace,
    sensitivity: Sensitivity,
    actor: &Actor,
) -> bool {
    if sensitivity > subject.clearance() {
        return false;
    }
    match engine.data_scope(subject, WRITE, actor) {
        DataScope::All => true,
        DataScope::None => false,
        DataScope::Restricted { org_unit_ids, .. } => {
            matches!(space, DocumentSpace::Unit(id) if org_unit_ids.contains(&id))
        }
    }
}
