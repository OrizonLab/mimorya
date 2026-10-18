//! Memory visibility rules. Memory items live in one of four spaces:
//!   - user:       personal working memory, visible to its owner only
//!   - org_unit:   team/department knowledge
//!   - tenant:     company-wide knowledge
//!   - experience: validated lessons and procedures used by the agent
//!
//! Shared spaces are readable by members of the unit (and of its sub-units: a
//! team member sees the department's knowledge) and by anyone whose
//! `memory:read` scope covers the unit. Writing to a shared space requires
//! `memory:write` on that scope. Personal memory is never shared through the
//! hierarchy: a manager cannot read a collaborator's personal memory.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::authz::{Actor, AuthorizationEngine, DataScope, Subject};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    User,
    OrgUnit,
    Tenant,
    Experience,
}

impl MemoryScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::OrgUnit => "org_unit",
            Self::Tenant => "tenant",
            Self::Experience => "experience",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::User, Self::OrgUnit, Self::Tenant, Self::Experience]
            .into_iter()
            .find(|s| s.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Preference,
    Fact,
    Procedure,
    Decision,
    Lesson,
    Episode,
}

impl MemoryKind {
    pub const ALL: [MemoryKind; 6] = [
        Self::Preference,
        Self::Fact,
        Self::Procedure,
        Self::Decision,
        Self::Lesson,
        Self::Episode,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::Fact => "fact",
            Self::Procedure => "procedure",
            Self::Decision => "decision",
            Self::Lesson => "lesson",
            Self::Episode => "episode",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == value)
    }
}

/// Shared org-unit memories visible to a subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitFilter {
    All,
    Only(HashSet<Uuid>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryReadFilter {
    /// Personal memory of this user is visible.
    pub owner_user_id: Uuid,
    pub org_units: UnitFilter,
    /// Company-wide and experience memories visible.
    pub shared: bool,
}

pub fn memory_read_filter(
    engine: &AuthorizationEngine,
    subject: &Subject,
    actor: &Actor,
) -> MemoryReadFilter {
    let owner_user_id = subject.user_id;
    match engine.data_scope(subject, "memory:read", actor) {
        DataScope::None => MemoryReadFilter {
            owner_user_id,
            org_units: UnitFilter::Only(HashSet::new()),
            shared: false,
        },
        DataScope::All => MemoryReadFilter {
            owner_user_id,
            org_units: UnitFilter::All,
            shared: true,
        },
        DataScope::Restricted { org_unit_ids, .. } => {
            let mut units = org_unit_ids;
            for unit_id in &subject.org_unit_ids {
                units.insert(*unit_id);
                units.extend(engine.org_tree().ancestors(*unit_id).iter().map(|a| a.id));
            }
            MemoryReadFilter {
                owner_user_id,
                org_units: UnitFilter::Only(units),
                shared: true,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryTarget {
    pub scope: MemoryScope,
    pub org_unit_id: Option<Uuid>,
}

/// Whether the subject (or an agent acting for them) may write into a memory space.
pub fn can_write_memory(
    engine: &AuthorizationEngine,
    subject: &Subject,
    target: MemoryTarget,
    actor: &Actor,
) -> bool {
    if target.scope == MemoryScope::User {
        return engine.has(subject, "agent:use", actor);
    }
    match engine.data_scope(subject, "memory:write", actor) {
        DataScope::All => true,
        DataScope::None => false,
        DataScope::Restricted { org_unit_ids, .. } => {
            // Tenant-wide and experience memories require a tenant-wide write scope.
            target.scope == MemoryScope::OrgUnit
                && target
                    .org_unit_id
                    .is_some_and(|unit| org_unit_ids.contains(&unit))
        }
    }
}
