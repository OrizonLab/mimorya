//! What the assistant remembers, as listed to a user: their own memories and
//! the shared ones they may read.

use mimorya_core::memory::MemorySource as StoredSource;
use mimorya_db::repositories::memory::MemoryRow;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::stored::stored_text;
use crate::http::iso;

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

/// `POST /memory`.
#[derive(Debug, Serialize, TS)]
pub struct MemoryCreated {
    pub id: Uuid,
}
