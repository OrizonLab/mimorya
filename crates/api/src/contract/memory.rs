//! What the assistant remembers, as listed to a user: their own memories and
//! the shared ones they may read; what a user adds or decides on a proposal.

use mimorya_core::memory::{MemoryKind as StoredKind, MemorySource as StoredSource};
use mimorya_db::repositories::memory::MemoryRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use validator::Validate;

use super::stored::stored_text;
use crate::http::{iso, trimmed};

/// The space a memory belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    /// One person's own memory.
    User,
    /// Shared by a unit of the organisation.
    OrgUnit,
    /// Shared by the whole company.
    Tenant,
    /// Lessons the assistant learnt from its runs.
    Experience,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Preference,
    Fact,
    Procedure,
    Decision,
    Lesson,
    Episode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    /// Waiting for validation by someone who may write the space.
    Proposed,
    Archived,
}

/// Where a memory comes from: written by hand, or by a run of the assistant.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct MemorySource {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
}

impl From<&StoredSource> for MemorySource {
    fn from(source: &StoredSource) -> Self {
        Self {
            kind: source.kind.clone(),
            id: source.id.clone(),
            label: source.label.clone(),
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: Uuid,
    pub scope: MemoryScope,
    /// Name of the unit sharing it, for a unit's memory.
    pub org_unit: Option<String>,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    pub status: MemoryStatus,
    pub source: MemorySource,
    /// How many times the assistant recalled it.
    pub use_count: i32,
    pub last_used_at: Option<String>,
    pub updated_at: String,
    /// The user may delete it (and validate it, when proposed).
    pub can_manage: bool,
}

impl MemoryItem {
    pub fn from_row(
        m: &MemoryRow,
        org_unit: Option<String>,
        can_manage: bool,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            id: m.id,
            scope: stored_text("memory scope", &m.scope)?,
            org_unit,
            kind: stored_text("memory kind", &m.kind)?,
            title: m.title.clone(),
            content: m.content.clone(),
            status: stored_text("memory status", &m.status)?,
            source: (&m.source).into(),
            use_count: m.use_count,
            last_used_at: m.last_used_at.as_ref().map(iso),
            updated_at: iso(&m.updated_at),
            can_manage,
        })
    }
}

/// `GET /memory`.
#[derive(Debug, Serialize, TS)]
pub struct MemoryList {
    pub items: Vec<MemoryItem>,
}

/// Where a user adds a memory: their own, their team's, the company's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MemorySpace {
    Personal,
    Team,
    Company,
}

/// What a user may write by hand (episodes come from runs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ManualMemoryKind {
    Preference,
    Fact,
    Procedure,
    Decision,
    Lesson,
}

impl From<ManualMemoryKind> for StoredKind {
    fn from(kind: ManualMemoryKind) -> Self {
        match kind {
            ManualMemoryKind::Preference => Self::Preference,
            ManualMemoryKind::Fact => Self::Fact,
            ManualMemoryKind::Procedure => Self::Procedure,
            ManualMemoryKind::Decision => Self::Decision,
            ManualMemoryKind::Lesson => Self::Lesson,
        }
    }
}

/// `POST /memory`.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct ManualMemory {
    pub space: MemorySpace,
    pub kind: ManualMemoryKind,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 3, max = 120))]
    pub title: String,
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 3, max = 4000))]
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ReviewDecision {
    Approve,
    Reject,
}

/// `POST /memory/{id}/review`: the decision on a proposed memory.
#[derive(Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
pub struct MemoryReview {
    pub decision: ReviewDecision,
}
