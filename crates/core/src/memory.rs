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

    pub const ALL: [MemoryScope; 4] = [Self::User, Self::OrgUnit, Self::Tenant, Self::Experience];

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == value)
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
        DataScope::Restricted { org_unit_ids, .. } => MemoryReadFilter {
            owner_user_id,
            org_units: UnitFilter::Only(readable_units(engine, subject, org_unit_ids)),
            shared: true,
        },
    }
}

/// The units whose shared knowledge (memory, documents) a subject reads
/// when their right covers `scoped` only: those, the units they belong to,
/// and the units above them.
pub fn readable_units(
    engine: &AuthorizationEngine,
    subject: &Subject,
    mut scoped: HashSet<Uuid>,
) -> HashSet<Uuid> {
    for unit_id in &subject.org_unit_ids {
        scoped.insert(*unit_id);
        scoped.extend(engine.org_tree().ancestors(*unit_id).iter().map(|a| a.id));
    }
    scoped
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryTarget {
    pub scope: MemoryScope,
    pub org_unit_id: Option<Uuid>,
}

/// Whether the subject (or an agent acting for them) may write into a memory
/// space directly. A run another system started writes into none: what it
/// would keep is proposed to its author, or what it was sent could settle in
/// their memory and lead their later runs (ADR-109).
pub fn can_write_memory(
    engine: &AuthorizationEngine,
    subject: &Subject,
    target: MemoryTarget,
    actor: &Actor,
) -> bool {
    if actor.started_by_hook() {
        return false;
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    /// Waiting for validation by someone holding `memory:write` on the space.
    Proposed,
    Archived,
}

impl MemoryStatus {
    pub const ALL: [MemoryStatus; 3] = [Self::Active, Self::Proposed, Self::Archived];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Proposed => "proposed",
            Self::Archived => "archived",
        }
    }
}

/// Where a memory comes from (`conversation`, `run`, `document` or `manual`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySource {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl MemorySource {
    pub fn manual(label: Option<&str>) -> Self {
        Self {
            kind: "manual".into(),
            id: None,
            label: label.map(Into::into),
        }
    }

    pub fn run(run_id: impl ToString) -> Self {
        Self {
            kind: "run".into(),
            id: Some(run_id.to_string()),
            label: None,
        }
    }
}
